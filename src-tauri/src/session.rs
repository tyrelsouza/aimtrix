//! Glue between the Matrix session and the app: starts and stops syncing,
//! broadcasts connection status and the Buddy List to windows, and handles a
//! revoked token.

use matrix_sdk::Client;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    matrix::{
        self, Session, Signed,
        rooms::{self, Buddy},
        sync::{self, Status},
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
    Some(signed.client)
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
