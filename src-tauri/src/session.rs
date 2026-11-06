//! Glue between the Matrix session and the app: starts and stops syncing,
//! broadcasts connection status and the Buddy List to windows, and handles a
//! revoked token.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use matrix_sdk::{
    Client,
    ruma::{OwnedRoomId, RoomId},
};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    matrix::{
        self, Session, Signed,
        crypto::{self, Recovery},
        members::{self, ChatRoom},
        presence::{self, Own, Tracker},
        rooms::{self, Buddy},
        sync::{self, Status},
        timeline::{self, Line},
        verify::{self, Flow, Step, Trust},
    },
    windows,
};

/// Event carrying the new [`Status`] to every window.
pub const STATUS_EVENT: &str = "sync:status";
/// Event carrying the whole Buddy List (`Vec<Buddy>`) to every window.
pub const BUDDY_LIST_EVENT: &str = "buddylist:update";
/// Event carrying the new [`Recovery`] state to every window.
pub const RECOVERY_EVENT: &str = "recovery:status";
/// Event carrying whether this device is verified ([`Trust`]) to every window.
pub const TRUST_EVENT: &str = "verification:trust";
/// Event carrying our own presence ([`Own`]) to every window.
pub const PRESENCE_EVENT: &str = "presence:update";
/// Event carrying the verification's new [`Step`] to the Verify window.
pub const STEP_EVENT: &str = "verification:step";

/// Start syncing `client` and make it the app's session.
pub async fn begin(app: &AppHandle, client: Client) -> Result<(), String> {
    let sync = sync::start(&client, {
        let app = app.clone();
        move |status| on_status(&app, status)
    })
    .await?;
    let presence = Tracker::new(client.clone(), presence::INTERVAL);
    let buddy_list = rooms::watch(sync.room_list_service(), presence.clone(), {
        let app = app.clone();
        move |buddies| on_buddies(&app, buddies)
    });
    let presence_events = {
        let app = app.clone();
        let presence = presence.clone();
        let mut changes = presence.subscribe();
        tauri::async_runtime::spawn(async move {
            while changes.changed().await.is_ok() {
                on_presence(&app, presence.own());
            }
        })
    };
    // Ask for the recovery key once per sign-on, not on every change.
    let asked = Arc::new(AtomicBool::new(false));
    let recovery = crypto::watch(&client, {
        let app = app.clone();
        move |state| on_recovery(&app, state, &asked)
    });
    let trust = verify::watch_trust(&client, {
        let app = app.clone();
        move |trust| on_trust(&app, trust)
    });
    let verification_requests = verify::on_requests(&client, {
        let app = app.clone();
        move |request| {
            let flow = Flow::incoming(request, {
                let app = app.clone();
                move |step| on_step(&app, step)
            });
            start_verification(&app, flow);
        }
    });
    *app.state::<Session>().signed.lock().await = Some(Signed {
        client,
        sync,
        buddy_list,
        presence,
        presence_events,
        recovery,
        trust,
        verification_requests,
    });
    Ok(())
}

/// Stop syncing and hand back the client, if signed on.
pub async fn end(app: &AppHandle) -> Option<Client> {
    let signed = app.state::<Session>().signed.lock().await.take()?;
    signed.buddy_list.abort();
    signed.presence_events.abort();
    signed.recovery.abort();
    signed.trust.abort();
    signed
        .client
        .remove_event_handler(signed.verification_requests);
    signed.sync.stop().await;
    let state = app.state::<Session>();
    *state.recovery.lock().unwrap() = Recovery::Unknown;
    *state.trust.lock().unwrap() = Trust::Unknown;
    *state.verification.lock().unwrap() = None;
    *state.verification_step.lock().unwrap() = None;
    app.state::<Session>().buddies.lock().unwrap().clear();
    // Dropping each watch stops it.
    app.state::<Session>().timelines.lock().unwrap().clear();
    app.state::<Session>().members.lock().unwrap().clear();
    for (label, window) in app.webview_windows() {
        if label.starts_with(windows::ROOM_LABEL_PREFIX)
            || label == windows::AWAY_LABEL
            || label == windows::ADD_BUDDY_LABEL
            || label == windows::RECOVERY_LABEL
            || label == windows::VERIFY_LABEL
            || label == windows::SETUP_RECOVERY_LABEL
        {
            let _ = window.close();
        }
    }
    Some(signed.client)
}

/// Follow a room's conversation for its window, replacing any earlier
/// watch (e.g. after a reload). Returns the current lines; changes go to
/// the window as `room:<key>:timeline_diff` events, and the names of who's
/// typing as `room:<key>:typing`.
pub async fn watch_timeline(
    app: &AppHandle,
    client: &Client,
    room_id: &str,
) -> Result<Vec<Line>, String> {
    let key = windows::room_key(room_id);
    let event = format!("room:{key}:timeline_diff");
    let typing_event = format!("room:{key}:typing");
    let label = format!("{}{key}", windows::ROOM_LABEL_PREFIX);
    let (lines, watching) = timeline::watch(
        client,
        room_id,
        {
            let (app, label) = (app.clone(), label.clone());
            move |diff| {
                if let Err(e) = app.emit_to(&label, &event, &diff) {
                    eprintln!("Couldn't send a timeline change to its window: {e}");
                }
            }
        },
        {
            let app = app.clone();
            move |names| {
                if let Err(e) = app.emit_to(&label, &typing_event, &names) {
                    eprintln!("Couldn't send who's typing to its window: {e}");
                }
            }
        },
    )
    .await?;
    // Replacing an earlier watch drops it, which stops it.
    app.state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .insert(room_id.to_owned(), Arc::new(watching));
    subscribe_open_rooms(app).await;
    Ok(lines)
}

/// Follow who's in a room and its topic for its Chat Room window,
/// replacing any earlier watch. Returns them as they are now; changes go to
/// the window as the whole member list in `room:<key>:members` events, and
/// the new topic (or null) in `room:<key>:topic`.
pub async fn watch_chat_room(
    app: &AppHandle,
    client: &Client,
    room_id: &str,
) -> Result<ChatRoom, String> {
    let key = windows::room_key(room_id);
    let members_event = format!("room:{key}:members");
    let topic_event = format!("room:{key}:topic");
    let label = format!("{}{key}", windows::ROOM_LABEL_PREFIX);
    let (first, watching) = members::watch(
        client,
        room_id,
        {
            let (app, label) = (app.clone(), label.clone());
            move |members| {
                if let Err(e) = app.emit_to(&label, &members_event, &members) {
                    eprintln!("Couldn't send who's in the room to its window: {e}");
                }
            }
        },
        {
            let app = app.clone();
            move |topic| {
                if let Err(e) = app.emit_to(&label, &topic_event, &topic) {
                    eprintln!("Couldn't send the topic to its window: {e}");
                }
            }
        },
    )
    .await?;
    app.state::<Session>()
        .members
        .lock()
        .unwrap()
        .insert(room_id.to_owned(), watching);
    Ok(first)
}

/// Stop following a room's conversation and members, once its window is
/// gone.
pub fn unwatch_timeline(app: &AppHandle, room_id: &str) {
    app.state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .remove(room_id);
    app.state::<Session>()
        .members
        .lock()
        .unwrap()
        .remove(room_id);
    let app = app.clone();
    tauri::async_runtime::spawn(async move { subscribe_open_rooms(&app).await });
}

/// Have sync send rooms with an open window in full. The room list alone
/// gets one event per room per sync, so a burst of messages leaves a gap
/// and the open conversation jumps back to just the newest few.
async fn subscribe_open_rooms(app: &AppHandle) {
    let room_ids: Vec<OwnedRoomId> = app
        .state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .keys()
        .filter_map(|id| RoomId::parse(id).ok())
        .collect();
    let service = match app.state::<Session>().signed.lock().await.as_ref() {
        Some(signed) => signed.sync.room_list_service(),
        None => return,
    };
    let room_ids: Vec<&RoomId> = room_ids.iter().map(|id| &**id).collect();
    service.set_room_subscriptions(&room_ids).await;
}

/// Send a message to a room whose window is open: plain text, plus
/// optional formatted text.
pub async fn send_message(
    app: &AppHandle,
    room_id: &str,
    body: &str,
    html: Option<&str>,
) -> Result<(), String> {
    watching(app, room_id)?.send(body, html).await
}

/// Tell a room whose window is open whether we're typing.
pub async fn set_typing(app: &AppHandle, room_id: &str, typing: bool) -> Result<(), String> {
    watching(app, room_id)?.set_typing(typing).await
}

/// Mark a room whose window is open read.
pub async fn mark_read(app: &AppHandle, room_id: &str) -> Result<(), String> {
    watching(app, room_id)?.mark_read().await
}

/// An image's thumbnail, in a room whose window is open.
pub async fn thumbnail(app: &AppHandle, room_id: &str, event_id: &str) -> Result<String, String> {
    watching(app, room_id)?.thumbnail(event_id).await
}

/// The suggested file name for an attachment, in a room whose window is
/// open.
pub async fn attachment_name(
    app: &AppHandle,
    room_id: &str,
    event_id: &str,
) -> Result<String, String> {
    watching(app, room_id)?.attachment_name(event_id).await
}

/// Save an attachment to `path`, in a room whose window is open.
pub async fn save_attachment(
    app: &AppHandle,
    room_id: &str,
    event_id: &str,
    path: std::path::PathBuf,
) -> Result<(), String> {
    watching(app, room_id)?
        .save_attachment(event_id, path)
        .await
}

/// Load older history in a room whose window is open. Returns whether the
/// start of the room was reached.
pub async fn load_older(app: &AppHandle, room_id: &str) -> Result<bool, String> {
    watching(app, room_id)?.load_older().await
}

/// The watch of an open room window's conversation. Cloned out so the map
/// isn't locked while it's used.
fn watching(app: &AppHandle, room_id: &str) -> Result<Arc<timeline::Watching>, String> {
    app.state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .get(room_id)
        .cloned()
        .ok_or_else(|| "That conversation isn't open.".to_owned())
}

fn on_status(app: &AppHandle, status: Status) {
    eprintln!("Sync status: {status:?}");
    *app.state::<Session>().status.lock().unwrap() = status.clone();
    if let Err(e) = app.emit(STATUS_EVENT, &status) {
        eprintln!("Couldn't send sync status to windows: {e}");
    }
    if status == Status::SignedOut {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { signed_out(&app).await });
    }
}

fn on_presence(app: &AppHandle, own: Own) {
    if let Err(e) = app.emit(PRESENCE_EVENT, &own) {
        eprintln!("Couldn't send your presence to windows: {e}");
    }
}

fn on_buddies(app: &AppHandle, buddies: Vec<Buddy>) {
    *app.state::<Session>().buddies.lock().unwrap() = buddies.clone();
    if let Err(e) = app.emit(BUDDY_LIST_EVENT, &buddies) {
        eprintln!("Couldn't send the Buddy List to windows: {e}");
    }
}

/// The server rejected our token (e.g. signed out from another client): forget
/// the session and go back to Sign On, explaining why.
async fn signed_out(app: &AppHandle) {
    end(app).await;
    match app.path().app_data_dir() {
        Ok(data_dir) => {
            if let Err(e) = matrix::forget(&data_dir).await {
                eprintln!("{e}");
            }
        }
        Err(e) => eprintln!("Couldn't find the session store to forget: {e}"),
    }
    if let Err(e) = windows::open_sign_on(app, true) {
        eprintln!("Couldn't open Sign On: {e}");
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.close();
    }
}

/// Share the new recovery state, and open Restore Messages the first time
/// this device turns out to need the recovery key.
fn on_recovery(app: &AppHandle, state: Recovery, asked: &AtomicBool) {
    *app.state::<Session>().recovery.lock().unwrap() = state;
    if let Err(e) = app.emit(RECOVERY_EVENT, state) {
        eprintln!("Couldn't send the recovery state to windows: {e}");
    }
    if state == Recovery::Incomplete
        && !asked.swap(true, Ordering::Relaxed)
        && let Err(e) = windows::open_recovery(app)
    {
        eprintln!("Couldn't open Restore Messages: {e}");
    }
}

/// Share whether this device is verified.
fn on_trust(app: &AppHandle, trust: Trust) {
    *app.state::<Session>().trust.lock().unwrap() = trust;
    if let Err(e) = app.emit(TRUST_EVENT, trust) {
        eprintln!("Couldn't send whether this device is verified: {e}");
    }
}

/// Share the verification's new step with the Verify window.
pub fn on_step(app: &AppHandle, step: Step) {
    *app.state::<Session>().verification_step.lock().unwrap() = Some(step.clone());
    if let Err(e) = app.emit_to(windows::VERIFY_LABEL, STEP_EVENT, &step) {
        eprintln!("Couldn't send the verification step: {e}");
    }
}

/// Make `flow` the verification in progress, replacing any earlier one,
/// and show it in the Verify window.
pub fn start_verification(app: &AppHandle, flow: Flow) {
    let state = app.state::<Session>();
    // The flow may already have reported its first step.
    *state.verification.lock().unwrap() = Some(Arc::new(flow));
    if let Err(e) = windows::open_verify(app) {
        eprintln!("Couldn't open Verify: {e}");
    }
}

/// Forget the verification (its window closed), cancelling it on both sides
/// if it's still going.
pub fn end_verification(app: &AppHandle) {
    let state = app.state::<Session>();
    let flow = state.verification.lock().unwrap().take();
    let step = state.verification_step.lock().unwrap().take();
    let finished = matches!(step, Some(Step::Done { .. } | Step::Cancelled { .. }));
    if let Some(flow) = flow
        && !finished
    {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = flow.cancel().await {
                eprintln!("{e}");
            }
        });
    }
}
