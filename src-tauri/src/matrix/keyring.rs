//! The saved session lives in the OS keyring (Secret Service on Linux), never
//! in plain files. These calls block, so async code runs them via
//! `spawn_blocking`.

use keyring::{Entry, Error};
use matrix_sdk::{
    authentication::{matrix::MatrixSession, oauth::UserSession},
    ruma::OwnedDeviceId,
};
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, MutexGuard, PoisonError};

const SERVICE: &str = "bike.tyrel.aimtrix";
const USER: &str = "session";

/// Everything needed to resume without signing on again. No `Debug`: it
/// holds the tokens.
#[derive(Clone, Serialize, Deserialize)]
pub struct SavedSession {
    pub homeserver: String,
    /// Encrypts the SQLite store, which will hold E2EE keys.
    pub store_passphrase: String,
    /// The store's folder in `stores/` (see `matrix::store_path`). Sessions
    /// saved before each had its own folder have none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(flatten)]
    pub auth: SavedAuth,
}

/// How the session signed on. Told apart by their fields, so sessions saved
/// before OAuth sign-on existed (just `session`) still load.
#[derive(Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SavedAuth {
    /// With a password (the Matrix login API).
    Password { session: MatrixSession },
    /// In the browser (OAuth). Its tokens expire and are refreshed, so this
    /// is saved again whenever they change.
    OAuth {
        /// What the server called AIMtrix when it registered.
        client_id: String,
        oauth: UserSession,
    },
}

impl SavedAuth {
    pub fn device_id(&self) -> &OwnedDeviceId {
        match self {
            Self::Password { session } => &session.meta.device_id,
            Self::OAuth { oauth, .. } => &oauth.meta.device_id,
        }
    }
}

/// Why the saved session couldn't be loaded.
#[derive(Debug)]
pub enum LoadError {
    /// It's there but can't be read: it will never work.
    Unreadable,
    /// The keyring couldn't be reached (locked, not running yet…). Trying
    /// again later may work.
    Keyring(String),
}

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, USER).map_err(|e| format!("Keyring unavailable: {e}"))
}

pub fn load() -> Result<Option<SavedSession>, LoadError> {
    match entry().map_err(LoadError::Keyring)?.get_password() {
        // Don't include the serde error: it can quote the token.
        Ok(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|_| LoadError::Unreadable),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(LoadError::Keyring(format!("Keyring error: {e}"))),
    }
}

impl LoadError {
    pub fn message(&self) -> String {
        match self {
            Self::Unreadable => "The saved session is unreadable.".to_owned(),
            Self::Keyring(message) => message.clone(),
        }
    }
}

/// Held while the saved session changes, so tokens refreshed during sign-off
/// can't be saved back after it's forgotten.
static CHANGING: Mutex<()> = Mutex::new(());

fn changing() -> MutexGuard<'static, ()> {
    CHANGING.lock().unwrap_or_else(PoisonError::into_inner)
}

pub fn save(saved: &SavedSession) -> Result<(), String> {
    let _changing = changing();
    write(saved)
}

/// Change the saved session, if there is one. `change` returns whether to
/// save it.
pub fn update(change: impl FnOnce(&mut SavedSession) -> bool) -> Result<(), String> {
    let _changing = changing();
    let Some(mut saved) = load().map_err(|e| e.message())? else {
        return Ok(());
    };
    if change(&mut saved) {
        write(&saved)
    } else {
        Ok(())
    }
}

fn write(saved: &SavedSession) -> Result<(), String> {
    let json = serde_json::to_string(saved).expect("SavedSession serializes");
    entry()?
        .set_password(&json)
        .map_err(|e| format!("Couldn't save session to keyring: {e}"))
}

/// Forget the saved session, whichever it is.
pub fn forget() -> Result<(), String> {
    let _changing = changing();
    delete()
}

/// Forget the saved session if it's the one for `device`. A sign-off still
/// finishing after you signed on again leaves the new session alone.
pub fn forget_device(device: &OwnedDeviceId) -> Result<(), String> {
    let _changing = changing();
    match load() {
        Ok(Some(saved)) if saved.auth.device_id() == device => delete(),
        Ok(_) => Ok(()),
        // Unreadable: it isn't anyone's session.
        Err(LoadError::Unreadable) => delete(),
        Err(LoadError::Keyring(message)) => Err(message),
    }
}

fn delete() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Couldn't remove session from keyring: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_saved_before_oauth_still_load() {
        let json = r#"{"homeserver":"http://localhost:8008/","store_passphrase":"p",
            "session":{"user_id":"@alice:localhost","device_id":"ABCDEFGHIJ",
            "access_token":"t"}}"#;
        let saved: SavedSession = serde_json::from_str(json).unwrap();
        assert!(matches!(saved.auth, SavedAuth::Password { .. }));
    }

    #[test]
    fn oauth_sessions_round_trip() {
        let json = r#"{"homeserver":"http://localhost:8013/","store_passphrase":"p",
            "client_id":"01ABC","oauth":{"user_id":"@alice:localhost","device_id":"ABCDEFGHIJ",
            "access_token":"t","refresh_token":"r"}}"#;
        let saved: SavedSession = serde_json::from_str(json).unwrap();
        let again: SavedSession =
            serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
        match again.auth {
            SavedAuth::OAuth { client_id, oauth } => {
                assert_eq!(client_id, "01ABC");
                assert_eq!(oauth.tokens.refresh_token.as_deref(), Some("r"));
            }
            SavedAuth::Password { .. } => panic!("read back as a password session"),
        }
    }
}
