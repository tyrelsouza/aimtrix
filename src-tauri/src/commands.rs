//! The `#[tauri::command]` surface the windows call.

use tauri::State;

use crate::matrix::{self, Session};

/// Sign on and return the user ID, e.g. `@alice:localhost`.
#[tauri::command]
pub async fn login(
    session: State<'_, Session>,
    homeserver: String,
    username: String,
    password: String,
) -> Result<String, String> {
    let client = matrix::login(&homeserver, &username, &password).await?;
    let user_id = client.user_id().expect("logged in client has a user ID").to_string();
    *session.0.lock().await = Some(client);
    Ok(user_id)
}
