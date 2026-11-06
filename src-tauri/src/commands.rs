//! The `#[tauri::command]` surface the windows call.

use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::{
    matrix::{self, Session, sync::Status},
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
