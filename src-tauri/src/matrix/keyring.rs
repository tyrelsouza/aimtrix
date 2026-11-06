//! The saved session lives in the OS keyring (Secret Service on Linux), never
//! in plain files. These calls block, so async code runs them via
//! `spawn_blocking`.

use keyring::{Entry, Error};
use matrix_sdk::authentication::matrix::MatrixSession;
use serde::{Deserialize, Serialize};

const SERVICE: &str = "bike.tyrel.aimtrix";
const USER: &str = "session";

/// Everything needed to resume without signing on again.
#[derive(Serialize, Deserialize)]
pub struct SavedSession {
    pub homeserver: String,
    /// Encrypts the SQLite store, which will hold E2EE keys.
    pub store_passphrase: String,
    pub session: MatrixSession,
}

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, USER).map_err(|e| format!("Keyring unavailable: {e}"))
}

pub fn load() -> Result<Option<SavedSession>, String> {
    match entry()?.get_password() {
        // Don't include the serde error: it can quote the token.
        Ok(json) => serde_json::from_str(&json)
            .map(Some)
            .map_err(|_| "The saved session is unreadable.".to_owned()),
        Err(Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Keyring error: {e}")),
    }
}

pub fn save(saved: &SavedSession) -> Result<(), String> {
    let json = serde_json::to_string(saved).expect("SavedSession serializes");
    entry()?.set_password(&json).map_err(|e| format!("Couldn't save session to keyring: {e}"))
}

pub fn forget() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Couldn't remove session from keyring: {e}")),
    }
}
