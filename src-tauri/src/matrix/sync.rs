//! Keeping the session in sync with the server, and reporting connection status.

use std::{sync::Arc, time::Duration};

use matrix_sdk::{Client, SessionChange};
use matrix_sdk_ui::{
    RoomListService,
    sync_service::{State, SyncService},
};
use serde::Serialize;
use tauri::async_runtime::{JoinHandle, spawn};

/// How long to wait before restarting after a sync error that isn't just
/// being offline (offline mode reconnects by itself).
const RETRY_AFTER: Duration = Duration::from_secs(5);

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
}

impl Syncing {
    pub fn room_list_service(&self) -> Arc<RoomListService> {
        self.service.room_list_service()
    }

    pub async fn stop(self) {
        // Stop reporting first so stopping doesn't show up as a status change.
        self.watcher.abort();
        self.service.stop().await;
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
        async move {
            loop {
                tokio::select! {
                    Some(state) = states.next() => match state {
                        State::Idle => on_status(Status::Connecting),
                        State::Running => on_status(Status::Online),
                        State::Offline => on_status(Status::Offline),
                        State::Terminated => {}
                        State::Error(e) => {
                            on_status(Status::Error(e.to_string()));
                            tokio::time::sleep(RETRY_AFTER).await;
                            service.start().await;
                        }
                    },
                    Ok(SessionChange::UnknownToken(_)) = session_changes.recv() => {
                        on_status(Status::SignedOut);
                        break;
                    }
                    else => break,
                }
            }
        }
    });

    service.start().await;
    Ok(Syncing { service, watcher })
}
