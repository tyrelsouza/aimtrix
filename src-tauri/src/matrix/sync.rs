//! Keeping the session in sync with the server, and reporting connection status.

use std::{sync::Arc, time::Duration};

use matrix_sdk::{
    Client, Room, SessionChange, event_handler::EventHandlerHandle,
    ruma::events::room::member::SyncRoomMemberEvent,
};
use matrix_sdk_ui::{
    RoomListService,
    sync_service::{State, SyncService},
};
use serde::Serialize;
use tauri::async_runtime::{JoinHandle, spawn};

/// How long to wait before restarting after a sync error that isn't just
/// being offline (offline mode reconnects by itself).
const RETRY_AFTER: Duration = Duration::from_secs(5);
/// How often [`Syncing::stop`] repeats its stop request.
const STOP_REPEAT: Duration = Duration::from_millis(100);

/// Connection status, as shown in the Buddy List status bar.
/// Serializes as `{"state": "online"}` or `{"state": "error", "message": "…"}`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(tag = "state", content = "message", rename_all = "kebab-case")]
pub enum Status {
    #[default]
    Connecting,
    Online,
    /// Can't reach the server; retrying automatically.
    Offline,
    /// Sync failed for another reason; retrying after a pause.
    Error(String),
    /// The server rejected the access token. The session must be forgotten.
    SignedOut,
}

/// A running sync. Call [`Syncing::stop`] before dropping the client.
pub struct Syncing {
    service: Arc<SyncService>,
    watcher: JoinHandle<()>,
    client: Client,
    members: EventHandlerHandle,
}

impl Syncing {
    pub fn room_list_service(&self) -> Arc<RoomListService> {
        self.service.room_list_service()
    }

    pub async fn stop(self) {
        // Stop reporting first so stopping doesn't show up as a status change.
        self.watcher.abort();
        self.client.remove_event_handler(self.members);
        // matrix-sdk 0.19 loses a stop request sent before the room list sync
        // starts listening for one, which happens on its first poll, and then
        // waits for it forever. Stopping right after starting (e.g. signing
        // off during launch) can do that, so repeat the request until the
        // stop finishes. Extra requests to a stopped sync are ignored.
        let room_list = self.service.room_list_service();
        let mut stopping = std::pin::pin!(self.service.stop());
        loop {
            tokio::select! {
                () = &mut stopping => break,
                () = tokio::time::sleep(STOP_REPEAT) => {
                    let _ = room_list.stop_sync();
                }
            }
        }
    }
}

/// Start syncing in the background. `on_status` is called on every change,
/// from a background task.
pub async fn start(
    client: &Client,
    on_status: impl Fn(Status) + Send + 'static,
) -> Result<Syncing, String> {
    let service = SyncService::builder(client.clone())
        .with_offline_mode()
        .build()
        .await
        .map_err(|e| format!("Couldn't start syncing: {e}"))?;
    let service = Arc::new(service);

    on_status(Status::Connecting);
    let mut states = service.state();
    let mut session_changes = client.subscribe_to_session_changes();
    let watcher = spawn({
        let service = service.clone();
        let client = client.clone();
        async move {
            // After an error, when to start again. A timer rather than a
            // sleep here, so a revoked token is still noticed meanwhile.
            let mut restart: Option<std::pin::Pin<Box<tokio::time::Sleep>>> = None;
            loop {
                tokio::select! {
                    () = async { restart.as_mut().unwrap().await }, if restart.is_some() => {
                        restart = None;
                        service.start().await;
                    }
                    Some(state) = states.next() => match state {
                        State::Idle => on_status(Status::Connecting),
                        State::Running => on_status(Status::Online),
                        State::Offline => on_status(Status::Offline),
                        State::Terminated => {}
                        State::Error(e) => {
                            let message = match super::sliding_sync_gone(&client).await {
                                Some(reason) => reason,
                                None => e.to_string(),
                            };
                            on_status(Status::Error(message));
                            restart = Some(Box::pin(tokio::time::sleep(RETRY_AFTER)));
                        }
                    },
                    Ok(change) = session_changes.recv() => match change {
                        SessionChange::UnknownToken(_) => {
                            on_status(Status::SignedOut);
                            break;
                        }
                        // OAuth access tokens expire; keep the new ones.
                        SessionChange::TokensRefreshed => {
                            if let Err(e) = super::save_refreshed_tokens(&client).await {
                                eprintln!("{e}");
                            }
                        }
                    },
                    else => break,
                }
            }
        }
    });

    let members = client.add_event_handler(notice_member_changes);
    service.start().await;
    Ok(Syncing {
        service,
        watcher,
        client: client.clone(),
        members,
    })
}

/// Sliding sync stores only the state in `required_state`, where servers
/// should also put the member events of the timeline. continuwuity leaves
/// them out, so the SDK would never learn who joined or left: Chat Room
/// member lists go stale, and room keys skip people who joined later. When a
/// member event doesn't match the stored member, have the SDK reload the
/// room's members from the server before it next needs them. On servers
/// that send the state it always matches, so this does nothing.
async fn notice_member_changes(event: SyncRoomMemberEvent, room: Room) {
    let user = event.state_key();
    let stored = match room.get_member_no_sync(user).await {
        Ok(member) => member.map(|m| m.membership().clone()),
        Err(_) => None,
    };
    if stored.as_ref() != Some(event.membership()) {
        room.mark_members_missing();
    }
}
