//! Who's online and who's away, for servers that have presence.
//!
//! Many servers turn presence off (matrix.org included), and sliding sync
//! doesn't carry it either way, so [`Tracker`] does it by hand: it sets us
//! online, reads that back to see whether the server keeps presence at all,
//! then keeps our presence (and away message) fresh and polls the buddies
//! the list asks about. A server without presence answers "offline" for
//! everyone, us included.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use matrix_sdk::{
    Client,
    ruma::{
        OwnedUserId, UserId,
        api::client::presence::{get_presence, set_presence},
        presence::PresenceState,
    },
};
use serde::Serialize;
use tauri::async_runtime::{JoinHandle, spawn};
use tokio::sync::{Notify, watch};

/// How often to poll buddies' presence.
pub const INTERVAL: Duration = Duration::from_secs(20);

/// How often to tell the server we're still here. Synapse signs a device
/// off after 30 s without a sync or presence update, and by default takes
/// one presence update per user every 10 s.
const HEARTBEAT: Duration = Duration::from_secs(20);

/// Longest away message, in characters. AIM allowed about 1024.
pub const MAX_AWAY_MESSAGE: usize = 1024;

/// A buddy's presence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Presence {
    Online,
    /// Signed on but away (`unavailable`).
    Away,
    Offline,
}

impl From<&PresenceState> for Presence {
    fn from(state: &PresenceState) -> Self {
        match state {
            PresenceState::Online => Self::Online,
            PresenceState::Unavailable => Self::Away,
            _ => Self::Offline,
        }
    }
}

/// A buddy's presence and status message (`status_msg`), which AIMtrix
/// shows as their away message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub presence: Presence,
    pub message: Option<String>,
}

impl Status {
    const OFFLINE: Self = Self {
        presence: Presence::Offline,
        message: None,
    };
}

/// Our own presence, for the windows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Own {
    /// Whether the server has presence. `None` until we know.
    pub supported: Option<bool>,
    /// Our away message while we're away.
    pub away: Option<String>,
}

#[derive(Default)]
struct State {
    own: Own,
    /// When we went away, while we are. Editing the away message keeps it.
    away_since: Option<SystemTime>,
    known: HashMap<OwnedUserId, Status>,
    /// Everyone the list has asked about, polled regularly.
    wanted: HashSet<OwnedUserId>,
}

pub struct Tracker {
    client: Client,
    state: Arc<Mutex<State>>,
    /// Told when someone new is wanted, to poll them right away.
    poll_now: Arc<Notify>,
    /// Told when we've just set our presence, so the next heartbeat waits.
    heartbeat_sent: Arc<Notify>,
    /// Bumped whenever support is found out, our away message changes or
    /// a buddy's status changes.
    changed: watch::Sender<()>,
    task: JoinHandle<()>,
}

impl Tracker {
    /// Start tracking, polling buddies every `interval` (normally
    /// [`INTERVAL`]). Polling stops when the tracker is dropped.
    pub fn new(client: Client, interval: Duration) -> Arc<Self> {
        let state = Arc::<Mutex<State>>::default();
        let poll_now = Arc::new(Notify::new());
        let heartbeat_sent = Arc::new(Notify::new());
        let (changed, _) = watch::channel(());
        let task = spawn(run(
            client.clone(),
            state.clone(),
            poll_now.clone(),
            heartbeat_sent.clone(),
            interval,
            changed.clone(),
        ));
        Arc::new(Self {
            client,
            state,
            poll_now,
            heartbeat_sent,
            changed,
            task,
        })
    }

    /// Notified whenever anything [`Tracker`] knows changes.
    pub fn subscribe(&self) -> watch::Receiver<()> {
        self.changed.subscribe()
    }

    /// Whether the server has presence. `None` until we know.
    pub fn supported(&self) -> Option<bool> {
        self.state.lock().unwrap().own.supported
    }

    pub fn own(&self) -> Own {
        self.state.lock().unwrap().own.clone()
    }

    /// Our away message and when we went away, while we're away.
    pub fn away(&self) -> Option<(String, SystemTime)> {
        let state = self.state.lock().unwrap();
        Some((state.own.away.clone()?, state.away_since?))
    }

    /// `user`'s status, or `None` if the server has no presence (or we
    /// don't know yet). Starts following `user`; until the first answer they
    /// count as offline.
    pub fn get(&self, user: &UserId) -> Option<Status> {
        let mut state = self.state.lock().unwrap();
        if state.own.supported != Some(true) {
            return None;
        }
        if state.wanted.insert(user.to_owned()) {
            self.poll_now.notify_one();
        }
        Some(state.known.get(user).cloned().unwrap_or(Status::OFFLINE))
    }

    /// Go away with `message`, or come back with `None`.
    pub async fn set_away(&self, message: Option<&str>) -> Result<(), String> {
        let message = match message.map(str::trim) {
            Some("") => return Err("Type an away message first.".to_owned()),
            Some(m) if m.chars().count() > MAX_AWAY_MESSAGE => {
                return Err(format!(
                    "Away messages can be up to {MAX_AWAY_MESSAGE} characters."
                ));
            }
            other => other.map(ToOwned::to_owned),
        };
        match self.supported() {
            Some(true) => {}
            Some(false) => return Err("This server doesn't support away messages.".to_owned()),
            None => return Err(
                "Still checking whether this server supports away messages; try again in a moment."
                    .to_owned(),
            ),
        }
        let own = self.client.user_id().ok_or("Not signed on.")?;
        // Store it first, so a heartbeat in between sends the new state.
        let (old, old_since) = {
            let mut state = self.state.lock().unwrap();
            let old_since = state.away_since;
            state.away_since = match (&state.own.away, &message) {
                (_, None) => None,
                (None, Some(_)) => Some(SystemTime::now()),
                (Some(_), Some(_)) => old_since,
            };
            (
                std::mem::replace(&mut state.own.away, message.clone()),
                old_since,
            )
        };
        if let Err(e) = set(&self.client, own, message.as_deref()).await {
            let mut state = self.state.lock().unwrap();
            state.own.away = old;
            state.away_since = old_since;
            return Err(format!("Couldn't update your away message: {e}"));
        }
        self.heartbeat_sent.notify_one();
        self.changed.send_replace(());
        Ok(())
    }
}

impl Drop for Tracker {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn run(
    client: Client,
    state: Arc<Mutex<State>>,
    poll_now: Arc<Notify>,
    heartbeat_sent: Arc<Notify>,
    interval: Duration,
    changed: watch::Sender<()>,
) {
    let Some(own) = client.user_id().map(ToOwned::to_owned) else {
        return;
    };
    // Keep trying until we know whether the server has presence.
    loop {
        if let Some(supported) = detect(&client, &own).await {
            state.lock().unwrap().own.supported = Some(supported);
            changed.send_replace(());
            if !supported {
                eprintln!("This server has no presence; buddies are sorted by activity.");
                return;
            }
            break;
        }
        tokio::time::sleep(interval).await;
    }
    tokio::join!(
        heartbeat(&client, &own, &state, &heartbeat_sent),
        poll(&client, &state, &poll_now, interval, &changed),
    );
}

/// Keep our presence and away message fresh on the server.
async fn heartbeat(client: &Client, own: &UserId, state: &Mutex<State>, sent: &Notify) {
    let mut ticks = tokio::time::interval_at(tokio::time::Instant::now() + HEARTBEAT, HEARTBEAT);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ticks.tick() => {
                let away = state.lock().unwrap().own.away.clone();
                if let Err(e) = set(client, own, away.as_deref()).await {
                    eprintln!("Couldn't tell the server we're still here: {e}");
                }
            }
            // Just set, so start the wait over.
            _ = sent.notified() => ticks.reset(),
        }
    }
}

/// Poll the buddies the list wants every `interval`, and new ones at once.
async fn poll(
    client: &Client,
    state: &Mutex<State>,
    poll_now: &Notify,
    interval: Duration,
    changed: &watch::Sender<()>,
) {
    loop {
        let wanted: Vec<_> = state.lock().unwrap().wanted.iter().cloned().collect();
        let mut any_changed = false;
        for user in wanted {
            // Leave anyone we can't read as they were.
            let Ok(status) = fetch(client, &user).await else {
                continue;
            };
            let mut state = state.lock().unwrap();
            any_changed |= state.known.get(&user) != Some(&status);
            state.known.insert(user, status);
        }
        if any_changed {
            changed.send_replace(());
        }
        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = poll_now.notified() => {}
        }
    }
}

/// Set us online and read it back. `Some(false)` if the server says we're
/// offline anyway (no presence); `None` if we couldn't tell.
async fn detect(client: &Client, own: &UserId) -> Option<bool> {
    if let Err(e) = set(client, own, None).await {
        // continuwuity refuses outright when presence is off.
        if status_of(&e) == Some(403) {
            eprintln!("The server refused to set presence: {e}");
            return Some(false);
        }
        eprintln!("Couldn't set presence: {e}");
        return None;
    }
    match fetch(client, own).await {
        Ok(status) => Some(status.presence != Presence::Offline),
        // continuwuity shows presence only to people sharing a room, us
        // included, so a new account with no rooms can't read its own yet.
        Err(e) if status_of(&e) == Some(404) => {
            eprintln!("Couldn't read our own presence yet, trying again later: {e}");
            None
        }
        // Some servers refuse presence requests outright when it's off.
        Err(e) => {
            eprintln!("Couldn't read presence, treating it as off: {e}");
            Some(false)
        }
    }
}

fn status_of(error: &matrix_sdk::HttpError) -> Option<u16> {
    error.as_client_api_error().map(|e| e.status_code.as_u16())
}

async fn fetch(client: &Client, user: &UserId) -> matrix_sdk::HttpResult<Status> {
    let response = client
        .send(get_presence::v3::Request::new(user.to_owned()))
        .await?;
    Ok(Status {
        presence: (&response.presence).into(),
        message: response.status_msg.filter(|m| !m.trim().is_empty()),
    })
}

/// Set us online, or away with `away_message`. Leaving the message out
/// clears it.
async fn set(
    client: &Client,
    own: &UserId,
    away_message: Option<&str>,
) -> matrix_sdk::HttpResult<()> {
    let mut request = set_presence::v3::Request::new(
        own.to_owned(),
        match away_message {
            Some(_) => PresenceState::Unavailable,
            None => PresenceState::Online,
        },
    );
    request.status_msg = away_message.map(ToOwned::to_owned);
    client.send(request).await.map(|_| ())
}

/// Tell the server we're gone, before signing off. Harmless on servers
/// without presence.
pub async fn sign_off(client: &Client) {
    let Some(own) = client.user_id() else {
        return;
    };
    let request = set_presence::v3::Request::new(own.to_owned(), PresenceState::Offline);
    if let Err(e) = client.send(request).await {
        eprintln!("Couldn't set presence to offline: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_is_away_and_unknown_is_offline() {
        assert_eq!(Presence::from(&PresenceState::Online), Presence::Online);
        assert_eq!(Presence::from(&PresenceState::Unavailable), Presence::Away);
        assert_eq!(Presence::from(&PresenceState::Offline), Presence::Offline);
        assert_eq!(
            Presence::from(&PresenceState::from("org.example.busy")),
            Presence::Offline
        );
    }

    #[test]
    fn own_presence_serializes_for_the_windows() {
        assert_eq!(
            serde_json::to_value(Own {
                supported: Some(true),
                away: Some("Out to lunch".to_owned()),
            })
            .unwrap(),
            serde_json::json!({ "supported": true, "away": "Out to lunch" })
        );
    }
}
