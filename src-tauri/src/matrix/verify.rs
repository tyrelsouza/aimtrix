//! Verifying this device against another of your sessions by comparing
//! emoji (SAS), and whether this device is verified.
//!
//! A [`Flow`] follows one verification, whichever side asked, and reports
//! each [`Step`]. Once both sides confirm, the other session signs this
//! device and sends over the secrets it's missing (the SDK does both).

use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use matrix_sdk::{
    Client,
    encryption::verification::{
        SasState, SasVerification, Verification, VerificationRequest, VerificationRequestState,
    },
    event_handler::EventHandlerHandle,
    ruma::events::{
        key::verification::cancel::CancelCode,
        key::verification::{VerificationMethod, request::ToDeviceKeyVerificationRequestEvent},
    },
};
use matrix_sdk_crypto::{CancelInfo, DeviceData};
use serde::Serialize;
use tauri::async_runtime::{JoinHandle, spawn};

/// Where a verification is, for the Verify window.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "step",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Step {
    /// We asked; waiting for another session to accept.
    Waiting,
    /// Another session (`device`, its name or ID) asked; accept or decline.
    Incoming { device: String },
    /// Accepted; the emoji are on their way. If the other session asked,
    /// it's expected to start the comparison, but `can_start` lets us do it
    /// (see [`Flow::start`]).
    Starting { device: String, can_start: bool },
    /// Compare these with the ones on the other session.
    Compare { device: String, emojis: Vec<Emoji> },
    /// We said they match; waiting for the other session to say so too.
    Confirmed { device: String },
    /// Both sides confirmed.
    Done { device: String },
    /// Stopped, by either side.
    Cancelled { reason: String },
}

/// One of the seven emoji to compare.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Emoji {
    pub symbol: String,
    pub description: String,
}

/// One verification in progress. Dropping it stops following it (but
/// doesn't cancel it; see [`Flow::cancel`]).
pub struct Flow {
    request: VerificationRequest,
    /// Set once the emoji comparison has started.
    sas: Arc<Mutex<Option<SasVerification>>>,
    task: JoinHandle<()>,
}

impl Flow {
    /// Ask your other sessions to verify this one.
    pub async fn request(
        client: &Client,
        on_step: impl Fn(Step) + Send + 'static,
    ) -> Result<Self, String> {
        let user_id = client.user_id().ok_or("Not signed on.")?;
        let identity = client
            .encryption()
            .request_user_identity(user_id)
            .await
            .map_err(|e| format!("Couldn't look up your sessions: {e}"))?
            .ok_or("Your account has no other session to verify with yet.")?;
        let request = identity
            .request_verification_with_methods(vec![VerificationMethod::SasV1])
            .await
            .map_err(|e| format!("Couldn't ask your other sessions: {e}"))?;
        Ok(Self::follow(request, on_step))
    }

    /// Follow a request another session sent.
    pub fn incoming(request: VerificationRequest, on_step: impl Fn(Step) + Send + 'static) -> Self {
        Self::follow(request, on_step)
    }

    fn follow(request: VerificationRequest, on_step: impl Fn(Step) + Send + 'static) -> Self {
        let sas = Arc::new(Mutex::new(None));
        let task = spawn(follow(request.clone(), sas.clone(), on_step));
        Self { request, sas, task }
    }

    /// Accept a request another session sent.
    pub async fn accept(&self) -> Result<(), String> {
        self.request
            .accept_with_methods(vec![VerificationMethod::SasV1])
            .await
            .map_err(|e| format!("Couldn't accept: {e}"))
    }

    /// Start comparing emoji, when the other session asked but waits for
    /// us to start.
    pub async fn start(&self) -> Result<(), String> {
        // The comparison shows up through the request's changes.
        self.request
            .start_sas()
            .await
            .map(|_| ())
            .map_err(|e| format!("Couldn't start comparing emoji: {e}"))
    }

    /// Say whether the emoji match the other session's.
    pub async fn confirm(&self, matched: bool) -> Result<(), String> {
        let sas = self
            .sas
            .lock()
            .unwrap()
            .clone()
            .ok_or("Nothing to compare yet.")?;
        if matched {
            sas.confirm().await
        } else {
            sas.mismatch().await
        }
        .map_err(|e| format!("Couldn't send your answer: {e}"))
    }

    /// Stop the verification on both sides.
    pub async fn cancel(&self) -> Result<(), String> {
        let sas = self.sas.lock().unwrap().clone();
        match sas {
            Some(sas) => sas.cancel().await,
            None => self.request.cancel().await,
        }
        .map_err(|e| format!("Couldn't cancel: {e}"))
    }
}

impl Drop for Flow {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// Follow the request until it turns into an emoji comparison, then that.
async fn follow(
    request: VerificationRequest,
    sas_slot: Arc<Mutex<Option<SasVerification>>>,
    on_step: impl Fn(Step) + Send + 'static,
) {
    let on_step = once_each(on_step);
    let mut changes = request.changes();
    let mut state = request.state();
    let sas = loop {
        match state {
            VerificationRequestState::Created { .. } => on_step(Step::Waiting),
            VerificationRequestState::Requested {
                other_device_data, ..
            } => on_step(Step::Incoming {
                device: device_name(&other_device_data),
            }),
            VerificationRequestState::Ready {
                other_device_data, ..
            } => {
                let device = device_name(&other_device_data);
                // Either side may start, but if both do at once the
                // verification stalls. So the side that asked starts, and the
                // other only does when its user says so.
                let we_asked = request.we_started();
                on_step(Step::Starting {
                    device: device.clone(),
                    can_start: !we_asked,
                });
                if we_asked {
                    match request.start_sas().await {
                        Ok(Some(sas)) => break (sas, device),
                        Ok(None) => {}
                        Err(e) => eprintln!("Couldn't start comparing emoji: {e}"),
                    }
                }
            }
            VerificationRequestState::Transitioned {
                verification: Verification::SasV1(sas),
            } => {
                let device = device_name(sas.other_device());
                break (sas, device);
            }
            VerificationRequestState::Transitioned { .. } => {}
            VerificationRequestState::Done => {
                // Only reached if the comparison ran elsewhere.
                return on_step(Step::Done {
                    device: String::new(),
                });
            }
            VerificationRequestState::Cancelled(info) => {
                return on_step(Step::Cancelled {
                    reason: cancel_reason(&info),
                });
            }
        }
        match changes.next().await {
            Some(next) => state = next,
            None => return,
        }
    };
    let (sas, device) = sas;
    *sas_slot.lock().unwrap() = Some(sas.clone());
    follow_sas(sas, device, on_step).await;
}

async fn follow_sas(sas: SasVerification, device: String, on_step: impl Fn(Step)) {
    let mut changes = sas.changes();
    let mut state = sas.state();
    loop {
        match state {
            SasState::Started { .. } if !sas.we_started() => {
                if let Err(e) = sas.accept().await {
                    eprintln!("Couldn't accept the emoji comparison: {e}");
                }
            }
            SasState::KeysExchanged {
                emojis: Some(emojis),
                ..
            } => on_step(Step::Compare {
                device: device.clone(),
                emojis: emojis
                    .emojis
                    .iter()
                    .map(|e| Emoji {
                        symbol: e.symbol.to_owned(),
                        description: e.description.to_owned(),
                    })
                    .collect(),
            }),
            SasState::KeysExchanged { emojis: None, .. } => {
                // Every client that supports SAS supports emoji; numbers
                // only would need another screen.
                let _ = sas.cancel().await;
            }
            SasState::Confirmed => on_step(Step::Confirmed {
                device: device.clone(),
            }),
            SasState::Done { .. } => return on_step(Step::Done { device }),
            SasState::Cancelled(info) => {
                return on_step(Step::Cancelled {
                    reason: cancel_reason(&info),
                });
            }
            _ => {}
        }
        match changes.next().await {
            Some(next) => state = next,
            None => return,
        }
    }
}

/// `on_step`, skipping a step that repeats the last one (the SDK reports
/// some states more than once).
fn once_each(on_step: impl Fn(Step)) -> impl Fn(Step) {
    let last = Mutex::new(None);
    move |step| {
        let mut last = last.lock().unwrap();
        if last.as_ref() != Some(&step) {
            *last = Some(step.clone());
            on_step(step);
        }
    }
}

/// The other session's name, or its device ID if it has none.
fn device_name(device: &DeviceData) -> String {
    device
        .display_name()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| device.device_id().to_string())
}

fn cancel_reason(info: &CancelInfo) -> String {
    match (info.cancel_code(), info.cancelled_by_us()) {
        (CancelCode::MismatchedSas, _) => {
            "The emoji didn't match, so this computer wasn't verified.".to_owned()
        }
        (CancelCode::User, true) => "You cancelled the verification.".to_owned(),
        (CancelCode::User, false) => "The other session cancelled the verification.".to_owned(),
        (CancelCode::Timeout, _) => "The verification timed out.".to_owned(),
        (CancelCode::Accepted, _) => "Another session answered the request.".to_owned(),
        _ => info.reason().to_owned(),
    }
}

/// Call `on_request` for each verification request from your own other
/// sessions. Requests from other people are ignored for now.
pub fn on_requests(
    client: &Client,
    on_request: impl Fn(VerificationRequest) + Send + Sync + Clone + 'static,
) -> EventHandlerHandle {
    client.add_event_handler(
        move |event: ToDeviceKeyVerificationRequestEvent, client: Client| {
            let on_request = on_request.clone();
            async move {
                if Some(&*event.sender) != client.user_id() {
                    return;
                }
                if let Some(request) = client
                    .encryption()
                    .get_verification_request(&event.sender, &event.content.transaction_id)
                    .await
                {
                    on_request(request);
                }
            }
        },
    )
}

/// Whether this device is verified.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Trust {
    /// Not checked yet.
    #[default]
    Unknown,
    /// The account has no cross-signing yet, so nothing can be verified.
    NoCrossSigning,
    /// Signed by the account's identity: other sessions trust this one.
    Verified,
    Unverified,
}

/// Whether this device is verified, now.
pub async fn trust(client: &Client) -> Trust {
    let encryption = client.encryption();
    let Some(user_id) = client.user_id() else {
        return Trust::Unknown;
    };
    match encryption.get_user_identity(user_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return Trust::NoCrossSigning,
        Err(_) => return Trust::Unknown,
    }
    match encryption.get_own_device().await {
        Ok(Some(device)) if device.is_cross_signed_by_owner() => Trust::Verified,
        Ok(Some(_)) => Trust::Unverified,
        _ => Trust::Unknown,
    }
}

/// Send this device's [`Trust`] to `on_change` now and whenever it changes,
/// until the returned task is aborted.
pub fn watch_trust(client: &Client, on_change: impl Fn(Trust) + Send + 'static) -> JoinHandle<()> {
    let client = client.clone();
    spawn(async move {
        let encryption = client.encryption();
        // Signatures on our device and identity changes both arrive here.
        let (Ok(mut devices), Ok(mut identities)) = (
            encryption.devices_stream().await,
            encryption.user_identities_stream().await,
        ) else {
            eprintln!("Couldn't follow whether this device is verified.");
            return;
        };
        encryption.wait_for_e2ee_initialization_tasks().await;
        let mut last = None;
        loop {
            let now = trust(&client).await;
            if last != Some(now) {
                last = Some(now);
                on_change(now);
            }
            tokio::select! {
                Some(_) = devices.next() => {}
                Some(_) = identities.next() => {}
                else => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_serialize_for_the_window() {
        assert_eq!(
            serde_json::to_value(Step::Compare {
                device: "Firefox".into(),
                emojis: vec![Emoji {
                    symbol: "🐶".into(),
                    description: "Dog".into()
                }],
            })
            .unwrap(),
            serde_json::json!({"step": "compare", "device": "Firefox",
                "emojis": [{"symbol": "🐶", "description": "Dog"}]})
        );
        assert_eq!(
            serde_json::to_value(Trust::NoCrossSigning).unwrap(),
            serde_json::json!("no-cross-signing")
        );
    }
}
