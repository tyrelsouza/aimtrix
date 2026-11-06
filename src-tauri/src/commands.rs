//! The `#[tauri::command]` surface the windows call.

use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::matrix::{self, Session};

/// Sign on, open the Buddy List, and close the Sign On window that called this.
/// Returns the user ID, e.g. `@alice:localhost`.
#[tauri::command]
pub async fn login(
    app: AppHandle,
    window: WebviewWindow,
    session: State<'_, Session>,
    homeserver: String,
    username: String,
    password: String,
) -> Result<String, String> {
    let client = matrix::login(&homeserver, &username, &password).await?;
    let user_id = client.user_id().expect("logged in client has a user ID").to_string();
    *session.0.lock().await = Some(client);

    open_buddy_list(&app).map_err(|e| e.to_string())?;
    window.close().map_err(|e| e.to_string())?;
    Ok(user_id)
}

/// The signed-on user ID, or `None` before sign-on.
#[tauri::command]
pub async fn whoami(session: State<'_, Session>) -> Result<Option<String>, String> {
    Ok(session.0.lock().await.as_ref().and_then(|c| c.user_id()).map(|u| u.to_string()))
}

/// The Buddy List is the `main` window. Focus it if it's already open.
fn open_buddy_list(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/".into()))
        .title("Buddy List")
        .inner_size(260.0, 540.0)
        .min_inner_size(200.0, 300.0)
        .build()?;
    Ok(())
}
