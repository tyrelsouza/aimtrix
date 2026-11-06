mod commands;
mod matrix;
mod windows;

use tauri::{AppHandle, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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
            commands::whoami
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Resume the saved session straight into the Buddy List, or show Sign On.
async fn startup(app: &AppHandle) -> tauri::Result<()> {
    let data_dir = app.path().app_data_dir()?;
    match matrix::resume(&data_dir).await {
        Ok(Some(client)) => {
            *app.state::<matrix::Session>().0.lock().await = Some(client);
            windows::open_buddy_list(app)
        }
        Ok(None) => windows::open_sign_on(app),
        Err(e) => {
            eprintln!("Couldn't resume the saved session, signing on again: {e}");
            if let Err(e) = matrix::forget(&data_dir).await {
                eprintln!("{e}");
            }
            windows::open_sign_on(app)
        }
    }
}
