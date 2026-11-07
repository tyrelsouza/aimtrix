//! The `#[tauri::command]` surface the windows call.

use std::path::PathBuf;

use matrix_sdk::Client;
use tauri::{AppHandle, Manager, State, WebviewWindow};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
use tauri_plugin_opener::OpenerExt;

use crate::{
    matrix::{
        self, Session,
        crypto::{self, Recovery, Setup},
        media,
        members::ChatRoom,
        presence::Own,
        rooms::{self, Buddy, Section},
        sync::Status,
        timeline::Line,
        upload,
        verify::{Flow, Step, Trust},
    },
    preferences, session, windows,
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

/// How `homeserver` lets people sign on: password, browser, or both.
#[tauri::command]
pub async fn sign_on_methods(homeserver: String) -> Result<matrix::Methods, String> {
    matrix::sign_on_methods(&homeserver).await
}

/// A sign-on in the browser that's waiting for the browser to come back;
/// sending cancels it.
#[derive(Default)]
pub struct BrowserSignOn(std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>);

/// Sign on in the browser (OAuth), then carry on like [`login`]. Waits until
/// the browser comes back, [`cancel_browser_login`] or a timeout.
#[tauri::command]
pub async fn login_with_browser(
    app: AppHandle,
    window: WebviewWindow,
    homeserver: String,
    pending: State<'_, BrowserSignOn>,
) -> Result<String, String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let (cancel, cancelled) = tokio::sync::oneshot::channel();
    // Starting again cancels a sign-on still waiting (its sender drops).
    *pending.0.lock().unwrap() = Some(cancel);
    let result = matrix::sign_on_with_browser(
        &data_dir,
        &homeserver,
        |url| {
            app.opener()
                .open_url(url.as_str(), None::<&str>)
                .map_err(|e| format!("Couldn't open your browser: {e}"))
        },
        async {
            let _ = cancelled.await;
        },
    )
    .await;
    {
        // Ours is done (its receiver dropped); leave a newer one waiting.
        let mut waiting = pending.0.lock().unwrap();
        if waiting.as_ref().is_some_and(|cancel| cancel.is_closed()) {
            waiting.take();
        }
    }
    let client = result?;
    let user_id = client
        .user_id()
        .expect("signed-on client has a user ID")
        .to_string();
    session::begin(&app, client).await?;

    windows::open_buddy_list(&app).map_err(|e| e.to_string())?;
    window.close().map_err(|e| e.to_string())?;
    Ok(user_id)
}

/// Stop waiting for the browser.
#[tauri::command]
pub fn cancel_browser_login(pending: State<'_, BrowserSignOn>) {
    if let Some(cancel) = pending.0.lock().unwrap().take() {
        let _ = cancel.send(());
    }
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

/// Send files from a room's window: `paths` (dropped on the window), or
/// ones picked in an Open dialog when `paths` is `None`. Returns `false` if
/// you cancelled the dialog. Progress arrives as `room:<key>:upload` events.
#[tauri::command]
pub async fn send_files(
    app: AppHandle,
    window: WebviewWindow,
    room_id: String,
    paths: Option<Vec<String>>,
) -> Result<bool, String> {
    let paths = match paths {
        Some(paths) => paths.into_iter().map(PathBuf::from).collect(),
        None => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            app.dialog()
                .file()
                .set_title("Send File")
                .set_parent(&window)
                .pick_files(move |paths| {
                    let _ = tx.send(paths);
                });
            let Some(picked) = rx.await.map_err(|e| e.to_string())? else {
                return Ok(false);
            };
            picked
                .into_iter()
                .map(|p| p.into_path().map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    let sources = paths.into_iter().map(upload::Source::File).collect();
    session::send_files(&app, &room_id, sources).await?;
    Ok(true)
}

/// Send a picture pasted into a room's window, after asking. The picture
/// is the raw request body; headers give the room (`x-room-id`) and its
/// type (`x-mime-type`). Returns `false` if you said no.
#[tauri::command]
pub async fn send_pasted(
    app: AppHandle,
    window: WebviewWindow,
    request: tauri::ipc::Request<'_>,
) -> Result<bool, String> {
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(ToOwned::to_owned)
            .ok_or_else(|| format!("Missing {name}."))
    };
    let (room_id, mime) = (header("x-room-id")?, header("x-mime-type")?);
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Nothing was pasted.".to_owned());
    };
    confirm_and_send_picture(&app, &window, &room_id, &mime, bytes.clone()).await
}

/// Send the picture on the system clipboard from a room's window, after
/// asking. For pastes the webview doesn't hand over. Returns `false` if
/// there's no picture or you said no.
#[tauri::command]
pub async fn paste_picture(
    app: AppHandle,
    window: WebviewWindow,
    room_id: String,
) -> Result<bool, String> {
    let picture = tauri::async_runtime::spawn_blocking(upload::clipboard_picture)
        .await
        .map_err(|e| e.to_string())??;
    match picture {
        Some(png) => confirm_and_send_picture(&app, &window, &room_id, "image/png", png).await,
        None => Ok(false),
    }
}

/// Ask whether to send a pasted picture, then send it.
async fn confirm_and_send_picture(
    app: &AppHandle,
    window: &WebviewWindow,
    room_id: &str,
    mime: &str,
    bytes: Vec<u8>,
) -> Result<bool, String> {
    let room_name = app
        .state::<Session>()
        .buddies
        .lock()
        .unwrap()
        .iter()
        .find(|b| b.room_id == room_id)
        .map(|b| b.name.clone())
        .unwrap_or_else(|| "this conversation".to_owned());
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .message(format!(
            "Send the pasted picture ({}) to {room_name}?",
            upload::describe_size(bytes.len() as u64)
        ))
        .title("Send Picture")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Send".to_owned(),
            "Cancel".to_owned(),
        ))
        .parent(window)
        .show(move |send| {
            let _ = tx.send(send);
        });
    if !rx.await.map_err(|e| e.to_string())? {
        return Ok(false);
    }
    let name = upload::pasted_name(mime);
    session::send_files(app, room_id, vec![upload::Source::Pasted { name, bytes }]).await?;
    Ok(true)
}

/// Stop sending files from a room's window.
#[tauri::command]
pub fn cancel_upload(app: AppHandle, room_id: String) {
    session::cancel_upload(&app, &room_id);
}

/// The signed-on client. Cloned so the session isn't locked while it's used.
async fn client(session: &Session) -> Result<Client, String> {
    let signed = session.signed.lock().await;
    signed
        .as_ref()
        .map(|s| s.client.clone())
        .ok_or_else(|| "Not signed on.".to_owned())
}

/// Whether the server has presence, and our away message. Changes arrive as
/// `presence:update` events.
#[tauri::command]
pub async fn own_presence(session: State<'_, Session>) -> Result<Own, String> {
    let signed = session.signed.lock().await;
    Ok(signed
        .as_ref()
        .map(|s| s.presence.own())
        .unwrap_or_default())
}

/// Go away with `message`, or come back with `None`.
#[tauri::command]
pub async fn set_away(session: State<'_, Session>, message: Option<String>) -> Result<(), String> {
    let presence = session
        .signed
        .lock()
        .await
        .as_ref()
        .map(|s| s.presence.clone())
        .ok_or("Not signed on.")?;
    presence.set_away(message.as_deref()).await
}

/// Open Preferences, or focus it.
#[tauri::command]
pub fn open_preferences(app: AppHandle) -> Result<(), String> {
    windows::open_preferences(&app).map_err(|e| e.to_string())
}

/// Open About AIMtrix, or focus it.
#[tauri::command]
pub fn open_about(app: AppHandle) -> Result<(), String> {
    windows::open_about(&app).map_err(|e| e.to_string())
}

/// Open AIMtrix's license (`"license"`) or the third-party licenses
/// (`"third-party"`), shipped with the app, in the default viewer.
#[tauri::command]
pub fn open_license(app: AppHandle, which: String) -> Result<(), String> {
    let file = match which.as_str() {
        "license" => "LICENSE.txt",
        "third-party" => "THIRD-PARTY-LICENSES.html",
        _ => return Err(format!("No license called {which}")),
    };
    let path = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join(file);
    app.opener()
        .open_path(path.to_string_lossy(), None::<&str>)
        .map_err(|e| format!("Couldn't open {file}: {e}"))
}

/// The preferences. Changes arrive as `preferences:update` events.
#[tauri::command]
pub fn get_preferences(store: State<'_, preferences::Store>) -> preferences::Preferences {
    store.get()
}

/// Save new preferences and share them with every window. Returns them as
/// saved (anything out of range is put back to its default).
#[tauri::command]
pub fn set_preferences(
    app: AppHandle,
    store: State<'_, preferences::Store>,
    preferences: preferences::Preferences,
) -> Result<preferences::Preferences, String> {
    store.set(&app, preferences)
}

/// Open Add Buddy, or focus it.
#[tauri::command]
pub fn open_add_buddy(app: AppHandle) -> Result<(), String> {
    windows::open_add_buddy(&app).map_err(|e| e.to_string())
}

/// Start a DM with whoever `buddy` names (a Matrix ID, or a name on our own
/// server), or find the one we already have. It shows up on the Buddy List
/// with the next update.
#[tauri::command]
pub async fn add_buddy(session: State<'_, Session>, buddy: String) -> Result<rooms::Added, String> {
    rooms::add_buddy(&client(&session).await?, &buddy).await
}

/// Block someone, after asking. Returns `false` if you said no. The Buddy
/// List and open conversations update by themselves.
#[tauri::command]
pub async fn block_buddy(
    app: AppHandle,
    window: WebviewWindow,
    session: State<'_, Session>,
    user_id: String,
    name: String,
) -> Result<bool, String> {
    let client = client(&session).await?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .message(format!(
            "Block {name}? You won't see their messages or invitations anywhere, \
             and they aren't told."
        ))
        .title("Block")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Block".to_owned(),
            "Cancel".to_owned(),
        ))
        .parent(&window)
        .show(move |block| {
            let _ = tx.send(block);
        });
    if !rx.await.map_err(|e| e.to_string())? {
        return Ok(false);
    }
    rooms::set_blocked(&client, &user_id, true).await?;
    Ok(true)
}

/// Unblock someone.
#[tauri::command]
pub async fn unblock_buddy(session: State<'_, Session>, user_id: String) -> Result<(), String> {
    rooms::set_blocked(&client(&session).await?, &user_id, false).await
}

/// Open Away Message, or focus it.
#[tauri::command]
pub fn open_away(app: AppHandle) -> Result<(), String> {
    windows::open_away(&app).map_err(|e| e.to_string())
}

/// Whether this device can read old messages from the key backup. Changes
/// arrive as `recovery:status` events.
#[tauri::command]
pub fn recovery_status(session: State<'_, Session>) -> Recovery {
    *session.recovery.lock().unwrap()
}

/// Open Restore Messages, or focus it.
#[tauri::command]
pub fn open_recovery(app: AppHandle) -> Result<(), String> {
    windows::open_recovery(&app).map_err(|e| e.to_string())
}

/// Unlock this device's secrets with the recovery key or passphrase; old
/// messages then decrypt as their keys download.
#[tauri::command]
pub async fn recover(session: State<'_, Session>, secret: String) -> Result<(), String> {
    crypto::recover(&client(&session).await?, &secret).await
}

/// Whether this device is verified. Changes arrive as `verification:trust`
/// events.
#[tauri::command]
pub fn device_trust(session: State<'_, Session>) -> Trust {
    *session.trust.lock().unwrap()
}

/// Ask your other sessions to verify this one, and open Verify.
#[tauri::command]
pub async fn verify_device(app: AppHandle, session: State<'_, Session>) -> Result<(), String> {
    let flow = Flow::request(&client(&session).await?, {
        let app = app.clone();
        move |step| session::on_step(&app, step)
    })
    .await?;
    session::start_verification(&app, flow);
    Ok(())
}

/// Where the verification in progress is, if there is one. Changes arrive
/// as `verification:step` events.
#[tauri::command]
pub fn verification_step(session: State<'_, Session>) -> Option<Step> {
    session.verification_step.lock().unwrap().clone()
}

/// Accept a verification request from another session.
#[tauri::command]
pub async fn accept_verification(session: State<'_, Session>) -> Result<(), String> {
    verification(&session)?.accept().await
}

/// Start comparing emoji, when the other session asked but waits for us.
#[tauri::command]
pub async fn start_verification(session: State<'_, Session>) -> Result<(), String> {
    verification(&session)?.start().await
}

/// Say whether the emoji match.
#[tauri::command]
pub async fn confirm_verification(
    session: State<'_, Session>,
    matched: bool,
) -> Result<(), String> {
    verification(&session)?.confirm(matched).await
}

/// Stop the verification in progress, on both sides.
#[tauri::command]
pub async fn cancel_verification(session: State<'_, Session>) -> Result<(), String> {
    verification(&session)?.cancel().await
}

fn verification(session: &Session) -> Result<std::sync::Arc<Flow>, String> {
    session
        .verification
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "There's no verification in progress.".to_owned())
}

/// Open Set Up Recovery, or focus it.
#[tauri::command]
pub fn open_setup_recovery(app: AppHandle) -> Result<(), String> {
    windows::open_setup_recovery(&app).map_err(|e| e.to_string())
}

/// Whether recovery can be set up on this device.
#[tauri::command]
pub async fn setup_recovery_check(session: State<'_, Session>) -> Result<Setup, String> {
    crypto::setup_check(&client(&session).await?).await
}

/// Set up a key backup and recovery. Returns the recovery key, to show once.
#[tauri::command]
pub async fn setup_recovery(session: State<'_, Session>) -> Result<String, String> {
    crypto::set_up(&client(&session).await?).await
}
