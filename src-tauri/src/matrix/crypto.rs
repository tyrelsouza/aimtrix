//! Getting a new device the keys to read old encrypted messages.
//!
//! An account with recovery set up keeps its secrets on the server, locked
//! with a recovery key (or a passphrase). [`recover`] unlocks them on this
//! device; then the SDK downloads every room key from the key backup and the
//! open conversations decrypt by themselves (see [`settings`]). [`set_up`]
//! gives an account without recovery a key backup and a recovery key.

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

/// Encryption settings for every client: give accounts without a
/// cross-signing identity one at sign-on (new accounts, mostly), and
/// download all room keys from the backup as soon as recovery unlocks it.
pub fn settings() -> EncryptionSettings {
    EncryptionSettings {
        auto_enable_cross_signing: true,
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

/// Whether [`set_up`] can run on this device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Setup {
    /// It can, now.
    Ready,
    /// The account's identity was made elsewhere and this device doesn't
    /// have its keys: verify this device with that session first.
    VerifyFirst,
    /// The account already has recovery (restore it if this device needs it).
    AlreadySetUp,
}

/// Whether recovery can be set up on this device.
pub async fn setup_check(client: &Client) -> Result<Setup, String> {
    let encryption = client.encryption();
    encryption.wait_for_e2ee_initialization_tasks().await;
    if encryption.recovery().state() != RecoveryState::Disabled
        && encryption
            .secret_storage()
            .is_enabled()
            .await
            .map_err(|e| format!("Couldn't check for recovery: {e}"))?
    {
        return Ok(Setup::AlreadySetUp);
    }
    let user_id = client.user_id().ok_or("Not signed on.")?;
    let has_identity = encryption
        .get_user_identity(user_id)
        .await
        .map_err(|e| format!("Couldn't look up your identity: {e}"))?
        .is_some();
    let has_keys = encryption
        .cross_signing_status()
        .await
        .is_some_and(|s| s.is_complete());
    Ok(if !has_identity || has_keys {
        Setup::Ready
    } else {
        Setup::VerifyFirst
    })
}

/// Set up a key backup and recovery, returning the new recovery key. It's
/// the only time the key is shown; it's never logged.
pub async fn set_up(client: &Client) -> Result<String, String> {
    match setup_check(client).await? {
        Setup::Ready => {}
        Setup::VerifyFirst => {
            return Err("Verify this computer with your other session first.".to_owned());
        }
        Setup::AlreadySetUp => return Err("Recovery is already set up.".to_owned()),
    }
    let encryption = client.encryption();
    // Normally done at sign-on; this covers sessions resumed from before.
    encryption
        .bootstrap_cross_signing_if_needed(None)
        .await
        .map_err(|e| format!("Couldn't set up encryption for your account: {e}"))?;
    // Backing up existing keys carries on in the background.
    encryption.recovery().enable().await.map_err(|e| match e {
        RecoveryError::BackupExistsOnServer => "Your account already has a key backup from \
                another app. Set up recovery there (for example in Element), then restore it \
                here."
            .to_owned(),
        e => format!("Couldn't set up recovery: {e}"),
    })
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
    fn setup_serializes_for_the_windows() {
        assert_eq!(
            serde_json::to_value(Setup::VerifyFirst).unwrap(),
            serde_json::json!("verify-first")
        );
    }

    #[test]
    fn recovery_serializes_for_the_windows() {
        assert_eq!(
            serde_json::to_value(Recovery::Incomplete).unwrap(),
            serde_json::json!("incomplete")
        );
        assert_eq!(Recovery::from(RecoveryState::Enabled), Recovery::Enabled);
    }
}
