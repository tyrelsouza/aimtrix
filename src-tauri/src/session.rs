//! Glue between the Matrix session and the app: starts and stops syncing,
//! broadcasts connection status and the Buddy List to windows, and handles a
//! revoked token.

use std::sync::Arc;

use matrix_sdk::Client;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    matrix::{
        self, Session, Signed,
        rooms::{self, Buddy},
        sync::{self, Status},
        timeline::{self, Line},
    },
    windows,
};

/// Event carrying the new [`Status`] to every window.
pub const STATUS_EVENT: &str = "sync:status";
/// Event carrying the whole Buddy List (`Vec<Buddy>`) to every window.
pub const BUDDY_LIST_EVENT: &str = "buddylist:update";

/// Start syncing `client` and make it the app's session.
pub async fn begin(app: &AppHandle, client: Client) -> Result<(), String> {
    let sync = sync::start(&client, {
        let app = app.clone();
        move |status| on_status(&app, status)
    })
    .await?;
    let buddy_list = rooms::watch(sync.room_list_service(), {
        let app = app.clone();
        move |buddies| on_buddies(&app, buddies)
    });
    *app.state::<Session>().signed.lock().await = Some(Signed {
        client,
        sync,
        buddy_list,
    });
    Ok(())
}

/// Stop syncing and hand back the client, if signed on.
pub async fn end(app: &AppHandle) -> Option<Client> {
    let signed = app.state::<Session>().signed.lock().await.take()?;
    signed.buddy_list.abort();
    signed.sync.stop().await;
    app.state::<Session>().buddies.lock().unwrap().clear();
    // Dropping each watch stops it.
    app.state::<Session>().timelines.lock().unwrap().clear();
    for (label, window) in app.webview_windows() {
        if label.starts_with(windows::ROOM_LABEL_PREFIX) {
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
    Ok(lines)
}

/// Stop following a room's conversation, once its window is gone.
pub fn unwatch_timeline(app: &AppHandle, room_id: &str) {
    app.state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .remove(room_id);
}

/// Send a plain-text message to a room whose window is open.
pub async fn send_message(app: &AppHandle, room_id: &str, body: &str) -> Result<(), String> {
    watching(app, room_id)?.send(body).await
}

/// Tell a room whose window is open whether we're typing.
pub async fn set_typing(app: &AppHandle, room_id: &str, typing: bool) -> Result<(), String> {
    watching(app, room_id)?.set_typing(typing).await
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
