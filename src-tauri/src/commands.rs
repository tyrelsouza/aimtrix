//! The `#[tauri::command]` surface the windows call.

use matrix_sdk::Client;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::DialogExt;

use crate::{
    matrix::{
        self, Session, media,
        members::ChatRoom,
        rooms::{self, Buddy, Section},
        sync::Status,
        timeline::Line,
    },
    session, windows,
};

/// Buddy Icons in the IM window are 48 px; 96 stays sharp on high-DPI
/// screens.
const ICON_PANE_SIZE: u32 = 96;

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

/// Open the IM or Chat Room window for a room on the Buddy List, or focus
/// it if it's already open.
#[tauri::command]
pub fn open_room(
    app: AppHandle,
    session: State<'_, Session>,
    room_id: String,
) -> Result<(), String> {
    let (name, kind) = {
        let buddies = session.buddies.lock().unwrap();
        match buddies.iter().find(|b| b.room_id == room_id) {
            Some(b) if b.section == Section::Invitations => {
                return Err("Accept the invitation to open this room.".to_owned());
            }
            Some(b) => (b.name.clone(), b.kind),
            None => return Err("That room isn't on your Buddy List.".to_owned()),
        }
    };
    windows::open_room(&app, &room_id, &name, kind).map_err(|e| e.to_string())
}

/// The lines of a room's conversation, for its window. Changes arrive as
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

/// Who's in a room and its topic, for its Chat Room window. Changes arrive
/// as `room:<key>:members` (the whole list) and `room:<key>:topic` events.
#[tauri::command]
pub async fn chat_room(
    app: AppHandle,
    session: State<'_, Session>,
    room_id: String,
) -> Result<ChatRoom, String> {
    session::watch_chat_room(&app, &client(&session).await?, &room_id).await
}

/// Send a message from a room's window. `body` is the plain text;
/// `html` is the formatted version, if any, and gets cleaned first.
#[tauri::command]
pub async fn send_message(
    app: AppHandle,
    room_id: String,
    body: String,
    html: Option<String>,
) -> Result<(), String> {
    if body.trim().is_empty() {
        return Err("Type a message first.".to_owned());
    }
    session::send_message(&app, &room_id, &body, html.as_deref()).await
}

/// Load older messages in a room's window; they arrive as
/// `room:<key>:timeline_diff` events. Returns whether the room's first
/// message was reached.
#[tauri::command]
pub async fn load_older(app: AppHandle, room_id: String) -> Result<bool, String> {
    session::load_older(&app, &room_id).await
}

/// Tell the room whether you're typing in its window. Others' typing
/// arrives as `room:<key>:typing` events with their names.
#[tauri::command]
pub async fn set_typing(app: AppHandle, room_id: String, typing: bool) -> Result<(), String> {
    session::set_typing(&app, &room_id, typing).await
}

/// Mark a room read from its window, while you're looking at it. The
/// Buddy List un-bolds it on the next update.
#[tauri::command]
pub async fn mark_read(app: AppHandle, room_id: String) -> Result<(), String> {
    session::mark_read(&app, &room_id).await
}

/// A room's Buddy Icon, big enough for the IM window's icon pane, as a
/// `data:` URL, or `None` for the default.
#[tauri::command]
pub async fn room_icon(
    session: State<'_, Session>,
    room_id: String,
) -> Result<Option<String>, String> {
    let buddy = session
        .buddies
        .lock()
        .unwrap()
        .iter()
        .find(|b| b.room_id == room_id)
        .cloned()
        .ok_or_else(|| "That room isn't on your Buddy List.".to_owned())?;
    rooms::icon(&client(&session).await?, &buddy, ICON_PANE_SIZE).await
}

/// Your own Buddy Icon as a `data:` URL, or `None` for the default.
#[tauri::command]
pub async fn my_icon(session: State<'_, Session>) -> Result<Option<String>, String> {
    let client = client(&session).await?;
    let mxc = match client.account().get_avatar_url().await {
        Ok(Some(mxc)) => mxc,
        Ok(None) => return Ok(None),
        Err(e) => return Err(format!("Couldn't look up your Buddy Icon: {e}")),
    };
    media::icon(&client, &mxc, ICON_PANE_SIZE)
        .await
        .map(Some)
        .map_err(|e| format!("Couldn't fetch your Buddy Icon: {e}"))
}

/// The thumbnail of an image message in a room's window, as a `data:`
/// URL.
#[tauri::command]
pub async fn image_thumbnail(
    app: AppHandle,
    room_id: String,
    event_id: String,
) -> Result<String, String> {
    session::thumbnail(&app, &room_id, &event_id).await
}

/// Ask where to save an attachment from a room's window, then save it.
/// Returns the file name it was saved as, or `None` if you cancelled.
#[tauri::command]
pub async fn save_attachment(
    app: AppHandle,
    window: WebviewWindow,
    room_id: String,
    event_id: String,
) -> Result<Option<String>, String> {
    let name = session::attachment_name(&app, &room_id, &event_id).await?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Save File")
        .set_file_name(&name)
        .set_parent(&window)
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let saved_as = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(name);
    session::save_attachment(&app, &room_id, &event_id, path).await?;
    Ok(Some(saved_as))
}

/// The signed-on client. Cloned so the session isn't locked while it's used.
async fn client(session: &Session) -> Result<Client, String> {
    let signed = session.signed.lock().await;
    signed
        .as_ref()
        .map(|s| s.client.clone())
        .ok_or_else(|| "Not signed on.".to_owned())
}
