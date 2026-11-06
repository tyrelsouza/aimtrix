//! Getting a new device the keys to read old encrypted messages.
//!
//! An account with recovery set up keeps its secrets on the server, locked
//! with a recovery key (or a passphrase). [`recover`] unlocks them on this
//! device; then the SDK downloads every room key from the key backup and the
//! open conversations decrypt by themselves (see [`settings`]).

use futures_util::StreamExt;
use matrix_sdk::{
    Client,
    encryption::{
        BackupDownloadStrategy, EncryptionSettings,
        recovery::{RecoveryError, RecoveryState},
        secret_storage::SecretStorageError,
    },
};
use serde::Serialize;
use tauri::async_runtime::{JoinHandle, spawn};

/// Encryption settings for every client: download all room keys from the
/// backup as soon as recovery unlocks it.
pub fn settings() -> EncryptionSettings {
    EncryptionSettings {
        backup_download_strategy: BackupDownloadStrategy::OneShot,
        ..Default::default()
    }
}

/// Whether this device can read old messages from the key backup.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Recovery {
    /// Not checked yet.
    #[default]
    Unknown,
    /// Recovery is set up and this device has its secrets.
    Enabled,
    /// The account has no recovery set up.
    Disabled,
    /// Recovery is set up, but this device needs the recovery key.
    Incomplete,
}

impl From<RecoveryState> for Recovery {
    fn from(state: RecoveryState) -> Self {
        match state {
            RecoveryState::Enabled => Self::Enabled,
            RecoveryState::Disabled => Self::Disabled,
            RecoveryState::Incomplete => Self::Incomplete,
            RecoveryState::Unknown => Self::Unknown,
        }
    }
}

/// Send the recovery state to `on_change` now and on every change, until
/// the returned task is aborted.
pub fn watch(client: &Client, on_change: impl Fn(Recovery) + Send + 'static) -> JoinHandle<()> {
    let mut states = client.encryption().recovery().state_stream();
    spawn(async move {
        while let Some(state) = states.next().await {
            on_change(state.into());
        }
    })
}

/// Unlock this device's secrets with the recovery key or passphrase. Keys
/// for old messages then download in the background.
pub async fn recover(client: &Client, secret: &str) -> Result<(), String> {
    // Recovery keys are often pasted with extra spaces or line breaks.
    let secret = secret.trim();
    if secret.is_empty() {
        return Err("Enter your recovery key or passphrase.".to_owned());
    }
    client
        .encryption()
        .recovery()
        .recover(secret)
        .await
        .map_err(|e| describe(&e))
}

/// The error for the window. Never includes the key itself.
fn describe(error: &RecoveryError) -> String {
    match error {
        RecoveryError::SecretStorage(SecretStorageError::SecretStorageKey(_)) => {
            "That recovery key or passphrase isn't right.".to_owned()
        }
        RecoveryError::SecretStorage(SecretStorageError::MissingKeyInfo { .. }) => {
            "This account has no recovery set up.".to_owned()
        }
        _ => format!("Couldn't restore your messages: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_serializes_for_the_windows() {
        assert_eq!(
            serde_json::to_value(Recovery::Incomplete).unwrap(),
            serde_json::json!("incomplete")
        );
        assert_eq!(Recovery::from(RecoveryState::Enabled), Recovery::Enabled);
    }
}
