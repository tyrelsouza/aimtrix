//! The `#[tauri::command]` surface the windows call.

use matrix_sdk::Client;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::{
    matrix::{
        self, Session,
        rooms::{self, Buddy, Section},
        sync::Status,
        timeline::Line,
    },
    session, windows,
};

/// Sign on, start syncing, open the Buddy List, and close the Sign On window
/// that called this. Returns the user ID, e.g. `@alice:localhost`.
#[tauri::command]
pub async fn login(
    app: AppHandle,
    window: WebviewWindow,
    homeserver: String,
    username: String,
    password: String,
) -> Result<String, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let client = matrix::sign_on(&data_dir, &homeserver, &username, &password).await?;
    let user_id = client
        .user_id()
        .expect("logged in client has a user ID")
        .to_string();
    session::begin(&app, client).await?;

    windows::open_buddy_list(&app).map_err(|e| e.to_string())?;
    window.close().map_err(|e| e.to_string())?;
    Ok(user_id)
}

/// Sign off, open Sign On, and close the window that called this.
#[tauri::command]
pub async fn logout(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    match session::end(&app).await {
        Some(client) => matrix::sign_off(&data_dir, client).await?,
        None => matrix::forget(&data_dir).await?,
    }

    windows::open_sign_on(&app, false).map_err(|e| e.to_string())?;
    window.close().map_err(|e| e.to_string())?;
    Ok(())
}

/// The signed-on user ID, or `None` before sign-on.
#[tauri::command]
pub async fn whoami(session: State<'_, Session>) -> Result<Option<String>, String> {
    let signed = session.signed.lock().await;
    Ok(signed
        .as_ref()
        .and_then(|s| s.client.user_id())
        .map(|u| u.to_string()))
}

/// The current connection status. Changes arrive as `sync:status` events.
#[tauri::command]
pub fn sync_status(session: State<'_, Session>) -> Status {
    session.status.lock().unwrap().clone()
}

/// The current Buddy List. Changes arrive as `buddylist:update` events.
#[tauri::command]
pub fn buddy_list(session: State<'_, Session>) -> Vec<Buddy> {
    session.buddies.lock().unwrap().clone()
}

/// Join the room of an invitation.
#[tauri::command]
pub async fn accept_invite(session: State<'_, Session>, room_id: String) -> Result<(), String> {
    rooms::accept(&client(&session).await?, &room_id).await
}

/// Reject an invitation.
#[tauri::command]
pub async fn decline_invite(session: State<'_, Session>, room_id: String) -> Result<(), String> {
    rooms::decline(&client(&session).await?, &room_id).await
}

/// Open the IM window for a room on the Buddy List, or focus it if it's
/// already open.
#[tauri::command]
pub fn open_room(
    app: AppHandle,
    session: State<'_, Session>,
    room_id: String,
) -> Result<(), String> {
    let name = {
        let buddies = session.buddies.lock().unwrap();
        match buddies.iter().find(|b| b.room_id == room_id) {
            Some(b) if b.section == Section::Invitations => {
                return Err("Accept the invitation to open this room.".to_owned());
            }
            Some(b) => b.name.clone(),
            None => return Err("That room isn't on your Buddy List.".to_owned()),
        }
    };
    windows::open_room(&app, &room_id, &name).map_err(|e| e.to_string())
}

/// The lines of a room's conversation, for its IM window. Changes arrive as
/// `room:<key>:timeline_diff` events, where `<key>` is the end of the
/// window's `room-<key>` label.
#[tauri::command]
pub async fn room_timeline(
    app: AppHandle,
    session: State<'_, Session>,
    room_id: String,
) -> Result<Vec<Line>, String> {
    session::watch_timeline(&app, &client(&session).await?, &room_id).await
}

/// The signed-on client. Cloned so the session isn't locked while it's used.
async fn client(session: &Session) -> Result<Client, String> {
    let signed = session.signed.lock().await;
    signed
        .as_ref()
        .map(|s| s.client.clone())
        .ok_or_else(|| "Not signed on.".to_owned())
}
