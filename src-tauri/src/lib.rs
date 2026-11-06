// matrix-sdk futures nest deeply enough to pass the default limit (the room
// window task in `matrix::timeline` does).
#![recursion_limit = "256"]

mod commands;
mod matrix;
mod session;
mod windows;

use tauri::{AppHandle, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(matrix::Session::default())
        .setup(|app| {
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
            commands::confirm_verification,
            commands::cancel_verification
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Resume the saved session straight into the Buddy List, or show Sign On.
async fn startup(app: &AppHandle) -> tauri::Result<()> {
    let data_dir = app.path().app_data_dir()?;
    let resumed = match matrix::resume(&data_dir).await {
        Ok(Some(client)) => session::begin(app, client).await.map(|()| true),
        Ok(None) => Ok(false),
        Err(e) => Err(e),
    };
    match resumed {
        Ok(true) => windows::open_buddy_list(app),
        Ok(false) => windows::open_sign_on(app, false),
        Err(e) => {
            eprintln!("Couldn't resume the saved session, signing on again: {e}");
            if let Err(e) = matrix::forget(&data_dir).await {
                eprintln!("{e}");
            }
            windows::open_sign_on(app, false)
        }
    }
}
