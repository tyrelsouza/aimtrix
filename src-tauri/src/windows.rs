//! Opening the app's windows. Each is focused instead of duplicated if it's
//! already open.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The Buddy List is the `main` window.
pub fn open_buddy_list(app: &AppHandle) -> tauri::Result<()> {
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

/// `signed_out` makes Sign On explain that the server ended the session.
pub fn open_sign_on(app: &AppHandle, signed_out: bool) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("signon") {
        return window.set_focus();
    }
    let url = if signed_out {
        "/signon?signed-out"
    } else {
        "/signon"
    };
    WebviewWindowBuilder::new(app, "signon", WebviewUrl::App(url.into()))
        .title("Sign On")
        .inner_size(250.0, 300.0)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}
