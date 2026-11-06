//! The single Matrix session owned by the Rust core: sign on, resume, sign off.

mod keyring;
pub mod sync;

use std::{
    io,
    path::{Path, PathBuf},
};

use matrix_sdk::{Client, ruma::api::error::ErrorKind};
use rand::distr::{Alphanumeric, SampleString};
use tauri::async_runtime::{Mutex, spawn_blocking};

use keyring::SavedSession;

/// App-wide session state, shared by every window.
#[derive(Default)]
pub struct Session {
    /// The signed-on client and its sync. `None` until sign-on.
    pub signed: Mutex<Option<Signed>>,
    /// The latest connection status, for windows that open after it changed.
    pub status: std::sync::Mutex<sync::Status>,
}

pub struct Signed {
    pub client: Client,
    pub sync: sync::Syncing,
}

fn store_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("matrix-store")
}

/// Sign on with a password, and save the session so the next launch can resume.
pub async fn sign_on(
    data_dir: &Path,
    homeserver: &str,
    username: &str,
    password: &str,
) -> Result<Client, String> {
    // A store belongs to one device, so every sign-on starts a fresh one.
    let store = store_dir(data_dir);
    remove_store(&store)?;
    let passphrase = Alphanumeric.sample_string(&mut rand::rng(), 32);

    let client = login(&store, &passphrase, homeserver, username, password).await?;
    let saved = SavedSession {
        homeserver: client.homeserver().to_string(),
        store_passphrase: passphrase,
        session: client.matrix_auth().session().expect("logged in client has a session"),
    };
    spawn_blocking(move || keyring::save(&saved)).await.map_err(|e| e.to_string())??;
    Ok(client)
}

/// Resume the saved session, if there is one. Works offline: the token is
/// only checked once syncing starts.
pub async fn resume(data_dir: &Path) -> Result<Option<Client>, String> {
    let Some(saved) = spawn_blocking(keyring::load).await.map_err(|e| e.to_string())?? else {
        return Ok(None);
    };
    restore(&store_dir(data_dir), &saved).await.map(Some)
}

/// Sign off: delete this device on the server, then forget it locally. Local
/// cleanup happens even if the server can't be reached.
pub async fn sign_off(data_dir: &Path, client: Client) -> Result<(), String> {
    if let Err(e) = client.logout().await {
        eprintln!("Server-side logout failed, forgetting the session anyway: {e}");
    }
    drop(client);
    forget(data_dir).await
}

/// Forget the saved session and its store without contacting the server.
pub async fn forget(data_dir: &Path) -> Result<(), String> {
    spawn_blocking(keyring::forget).await.map_err(|e| e.to_string())??;
    remove_store(&store_dir(data_dir))
}

/// Log in with a password into a new store at `store`.
///
/// `homeserver` is a server name (discovered via `.well-known`) or a full URL.
async fn login(
    store: &Path,
    passphrase: &str,
    homeserver: &str,
    username: &str,
    password: &str,
) -> Result<Client, String> {
    let client = Client::builder()
        .server_name_or_homeserver_url(homeserver)
        .sqlite_store(store, Some(passphrase))
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

/// Reopen the store at `store` and restore the saved session into it.
async fn restore(store: &Path, saved: &SavedSession) -> Result<Client, String> {
    let client = Client::builder()
        .homeserver_url(&saved.homeserver)
        .sqlite_store(store, Some(&saved.store_passphrase))
        .build()
        .await
        .map_err(|e| format!("Couldn't open the session store: {e}"))?;
    client
        .restore_session(saved.session.clone())
        .await
        .map_err(|e| format!("Couldn't restore the session: {e}"))?;
    Ok(client)
}

fn remove_store(store: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(store) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Couldn't clear the old session store: {e}")),
    }
}

/// Tests marked `ignore` need the dev homeserver: `docker compose -f
/// dev/docker-compose.yml up -d`, `python3 dev/seed.py`, then
/// `cargo test -- --include-ignored`. None of them touch the OS keyring.
#[cfg(test)]
mod tests {
    use super::*;

    const HS: &str = "http://localhost:8008";
    const PASS: &str = "store-passphrase";

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn login_returns_user_id() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "alice", "aimtrix-test").await.unwrap();
        assert_eq!(client.user_id().unwrap(), "@alice:localhost");
        // Don't leave a device behind on the test account.
        client.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn login_rejects_bad_password() {
        let dir = tempfile::tempdir().unwrap();
        let err = login(dir.path(), PASS, HS, "alice", "wrong").await.unwrap_err();
        assert_eq!(err, "Incorrect screen name or password.");
    }

    #[tokio::test]
    async fn login_rejects_unreachable_homeserver() {
        let dir = tempfile::tempdir().unwrap();
        let err = login(dir.path(), PASS, "http://127.0.0.1:9", "alice", "x").await.unwrap_err();
        assert!(err.starts_with("Couldn't reach homeserver"), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn restore_resumes_session_from_store() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "bob", "aimtrix-test").await.unwrap();
        let mut saved = SavedSession {
            homeserver: client.homeserver().to_string(),
            store_passphrase: PASS.to_owned(),
            session: client.matrix_auth().session().unwrap(),
        };
        drop(client);

        saved.store_passphrase = "wrong".to_owned();
        assert!(restore(dir.path(), &saved).await.is_err(), "wrong passphrase must not open the store");

        saved.store_passphrase = PASS.to_owned();
        let client = restore(dir.path(), &saved).await.unwrap();
        // A real server round trip proves the restored token works.
        assert_eq!(client.whoami().await.unwrap().user_id, "@bob:localhost");
        client.logout().await.unwrap();
    }

    /// Wait up to 20 s for `want` to arrive on `rx`.
    async fn wait_for(rx: &mut tokio::sync::mpsc::UnboundedReceiver<sync::Status>, want: sync::Status) {
        let found = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            while let Some(status) = rx.recv().await {
                if status == want {
                    return;
                }
            }
            panic!("status stream ended before {want:?}");
        });
        found.await.unwrap_or_else(|_| panic!("timed out waiting for {want:?}"));
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn sync_goes_online() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "carol", "aimtrix-test").await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let syncing = sync::start(&client, move |s| tx.send(s).unwrap()).await.unwrap();

        wait_for(&mut rx, sync::Status::Connecting).await;
        wait_for(&mut rx, sync::Status::Online).await;

        syncing.stop().await;
        client.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn sync_reports_revoked_token_as_signed_out() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "carol", "aimtrix-test").await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let syncing = sync::start(&client, move |s| tx.send(s).unwrap()).await.unwrap();
        wait_for(&mut rx, sync::Status::Online).await;

        // Revoke the token from "another client" sharing the same session.
        let other_dir = tempfile::tempdir().unwrap();
        let saved = SavedSession {
            homeserver: client.homeserver().to_string(),
            store_passphrase: PASS.to_owned(),
            session: client.matrix_auth().session().unwrap(),
        };
        restore(other_dir.path(), &saved).await.unwrap().logout().await.unwrap();

        wait_for(&mut rx, sync::Status::SignedOut).await;
        syncing.stop().await;
    }

    #[test]
    fn remove_store_ignores_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        remove_store(&dir.path().join("nope")).unwrap();
        remove_store(dir.path()).unwrap();
        assert!(!dir.path().exists());
    }
}
