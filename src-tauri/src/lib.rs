mod commands;
mod matrix;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(matrix::Session::default())
        .invoke_handler(tauri::generate_handler![commands::login])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
