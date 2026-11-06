//! The single Matrix session owned by the Rust core.

use matrix_sdk::{Client, ruma::api::error::ErrorKind};
use tauri::async_runtime::Mutex;

/// The signed-on client, shared by every window. `None` until sign-on.
#[derive(Default)]
pub struct Session(pub Mutex<Option<Client>>);

/// Log in with a username and password.
///
/// `homeserver` is a server name (discovered via `.well-known`) or a full URL.
/// The client has no persistent store yet; that comes with session restore.
pub async fn login(homeserver: &str, username: &str, password: &str) -> Result<Client, String> {
    let client = Client::builder()
        .server_name_or_homeserver_url(homeserver)
        .build()
        .await
        .map_err(|e| format!("Couldn't reach homeserver: {e}"))?;

    client
        .matrix_auth()
        .login_username(username, password)
        .initial_device_display_name("AIMtrix")
        .await
        .map_err(|e| match e.client_api_error_kind() {
            Some(ErrorKind::Forbidden) => "Incorrect screen name or password.".to_owned(),
            Some(ErrorKind::UserDeactivated) => "This account has been deactivated.".to_owned(),
            _ => format!("Sign on failed: {e}"),
        })?;

    Ok(client)
}

/// These need the dev homeserver: `docker compose -f dev/docker-compose.yml up -d`,
/// `python3 dev/seed.py`, then `cargo test -- --ignored`.
#[cfg(test)]
mod tests {
    use super::*;

    const HS: &str = "http://localhost:8008";

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn login_returns_user_id() {
        let client = login(HS, "alice", "aimtrix-test").await.unwrap();
        assert_eq!(client.user_id().unwrap(), "@alice:localhost");
        // Don't leave a device behind on the test account.
        client.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn login_rejects_bad_password() {
        let err = login(HS, "alice", "wrong").await.unwrap_err();
        assert_eq!(err, "Incorrect screen name or password.");
    }

    #[tokio::test]
    async fn login_rejects_unreachable_homeserver() {
        let err = login("http://127.0.0.1:9", "alice", "x").await.unwrap_err();
        assert!(err.starts_with("Couldn't reach homeserver"), "{err}");
    }
}
