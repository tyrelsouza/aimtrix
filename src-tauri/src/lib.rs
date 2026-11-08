// matrix-sdk futures nest deeply enough to pass the default limit (the room
// window task in `matrix::timeline` does).
#![recursion_limit = "256"]

mod commands;
mod matrix;
mod preferences;
mod session;
mod tray;
mod windows;

use tauri::{AppHandle, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // First, so a second launch stops before doing anything else: two
        // copies would share one store and one device, which corrupts the
        // encryption state. Launching again shows the running one instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::open(app.clone());
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(matrix::Session::default())
        .manage(commands::BrowserSignOn::default())
        .manage(tray::Tray::default())
        .setup(|app| {
            app.manage(preferences::Store::open(app.handle())?);
            if let Err(e) = tray::create(app.handle()) {
                eprintln!("Couldn't add the tray icon: {e}");
            }
            let app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = startup(&app).await {
                    // With no window open there's nothing to show an error in.
                    eprintln!("Couldn't open a window: {e}");
                    app.exit(1);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::login,
            commands::sign_on_methods,
            commands::login_with_browser,
            commands::cancel_browser_login,
            commands::logout,
            commands::whoami,
            commands::sync_status,
            commands::buddy_list,
            commands::accept_invite,
            commands::decline_invite,
            commands::open_room,
            commands::room_timeline,
            commands::chat_room,
            commands::send_message,
            commands::load_older,
            commands::set_typing,
            commands::mark_read,
            commands::my_icon,
            commands::room_icon,
            commands::image_thumbnail,
            commands::save_attachment,
            commands::send_files,
            commands::send_pasted,
            commands::paste_picture,
            commands::cancel_upload,
            commands::open_add_buddy,
            commands::open_preferences,
            commands::open_about,
            commands::tray_available,
            commands::open_license,
            commands::get_preferences,
            commands::set_preferences,
            commands::block_buddy,
            commands::unblock_buddy,
            commands::add_buddy,
            commands::own_presence,
            commands::set_away,
            commands::open_away,
            commands::recovery_status,
            commands::open_recovery,
            commands::recover,
            commands::open_setup_recovery,
            commands::setup_recovery_check,
            commands::setup_recovery,
            commands::device_trust,
            commands::verify_device,
            commands::verification_step,
            commands::accept_verification,
            commands::start_verification,
            commands::confirm_verification
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // macOS: like other Mac apps, keep running when the last window
            // closes (Cmd+Q still quits), and reopen from the Dock.
            #[cfg(target_os = "macos")]
            match event {
                tauri::RunEvent::ExitRequested {
                    code: None, api, ..
                } => api.prevent_exit(),
                tauri::RunEvent::Reopen {
                    has_visible_windows: false,
                    ..
                } => tray::open(app.clone()),
                _ => {}
            }
            #[cfg(not(target_os = "macos"))]
            let _ = (app, event);
        });
}

/// Resume the saved session straight into the Buddy List, or show Sign On.
/// If resuming fails, Sign On says why; the saved session is kept, since
/// the cause may be temporary (a locked keyring, say), and it holds the keys
/// to old encrypted messages. Signing on again replaces it.
async fn startup(app: &AppHandle) -> tauri::Result<()> {
    let data_dir = app.path().app_data_dir()?;
    let resumed = match matrix::resume(&data_dir).await {
        Ok(Some(account)) => session::begin(app, session::Start::Resumed(account))
            .await
            .map(|()| true),
        Ok(None) => Ok(false),
        Err(e) => Err(e),
    };
    match resumed {
        Ok(true) => windows::open_buddy_list(app),
        Ok(false) => windows::open_sign_on(app, windows::SignOnNotice::None),
        Err(e) => {
            eprintln!("Couldn't resume the saved session: {e}");
            windows::open_sign_on(app, windows::SignOnNotice::ResumeFailed(e))
        }
    }
}
