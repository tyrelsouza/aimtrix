//! Who's online, for servers that have presence.
//!
//! Many servers turn presence off (matrix.org included), and sliding sync
//! doesn't carry it either way, so [`Tracker`] does it by hand: it sets us
//! online, reads that back to see whether the server keeps presence at all,
//! then keeps us online and polls the buddies the list asks about. A server
//! without presence answers "offline" for everyone, us included.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::Duration,
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
use tokio::sync::{Notify, mpsc::UnboundedSender};

/// How often to poll buddies and tell the server we're still here. Synapse
/// signs a device off after 30 s without a sync or presence update.
pub const INTERVAL: Duration = Duration::from_secs(20);

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

#[derive(Default)]
struct State {
    /// Whether the server has presence. `None` until we know.
    supported: Option<bool>,
    known: HashMap<OwnedUserId, Presence>,
    /// Everyone the list has asked about, polled every [`INTERVAL`].
    wanted: HashSet<OwnedUserId>,
}

pub struct Tracker {
    state: Arc<Mutex<State>>,
    /// Told when someone new is wanted, to poll them right away.
    poll_now: Arc<Notify>,
    task: JoinHandle<()>,
}

impl Tracker {
    /// Start tracking. `changed` is told whenever support is found out or
    /// someone's presence changes, so the list can be rebuilt. Polling stops
    /// when the tracker is dropped.
    pub fn new(client: Client, interval: Duration, changed: UnboundedSender<()>) -> Self {
        let state = Arc::<Mutex<State>>::default();
        let poll_now = Arc::new(Notify::new());
        let task = spawn(run(
            client,
            state.clone(),
            poll_now.clone(),
            interval,
            changed,
        ));
        Self {
            state,
            poll_now,
            task,
        }
    }

    /// Whether the server has presence. `None` until we know.
    pub fn supported(&self) -> Option<bool> {
        self.state.lock().unwrap().supported
    }

    /// `user`'s presence, or `None` if the server has no presence (or we
    /// don't know yet). Starts following `user`; until the first answer they
    /// count as offline.
    pub fn get(&self, user: &UserId) -> Option<Presence> {
        let mut state = self.state.lock().unwrap();
        if state.supported != Some(true) {
            return None;
        }
        if state.wanted.insert(user.to_owned()) {
            self.poll_now.notify_one();
        }
        Some(state.known.get(user).copied().unwrap_or(Presence::Offline))
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
    interval: Duration,
    changed: UnboundedSender<()>,
) {
    let Some(own) = client.user_id().map(ToOwned::to_owned) else {
        return;
    };
    // Keep trying until we know whether the server has presence.
    loop {
        if let Some(supported) = detect(&client, &own).await {
            state.lock().unwrap().supported = Some(supported);
            let _ = changed.send(());
            if !supported {
                eprintln!("This server has no presence; buddies are sorted by activity.");
                return;
            }
            break;
        }
        tokio::time::sleep(interval).await;
    }

    let mut heartbeat = tokio::time::interval_at(tokio::time::Instant::now() + interval, interval);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        let wanted: Vec<_> = state.lock().unwrap().wanted.iter().cloned().collect();
        let mut any_changed = false;
        for user in wanted {
            // Leave anyone we can't read as they were.
            let Ok(presence) = fetch(&client, &user).await else {
                continue;
            };
            let old = state.lock().unwrap().known.insert(user, presence);
            any_changed |= old != Some(presence);
        }
        if any_changed {
            let _ = changed.send(());
        }
        tokio::select! {
            _ = heartbeat.tick() => {
                if let Err(e) = set(&client, &own, PresenceState::Online).await {
                    eprintln!("Couldn't tell the server we're online: {e}");
                }
            }
            _ = poll_now.notified() => {}
        }
    }
}

/// Set us online and read it back. `Some(false)` if the server says we're
/// offline anyway (no presence); `None` if we couldn't tell.
async fn detect(client: &Client, own: &UserId) -> Option<bool> {
    if let Err(e) = set(client, own, PresenceState::Online).await {
        eprintln!("Couldn't set presence: {e}");
        return None;
    }
    match fetch(client, own).await {
        Ok(presence) => Some(presence != Presence::Offline),
        // Some servers refuse presence requests outright when it's off.
        Err(e) => {
            eprintln!("Couldn't read presence, treating it as off: {e}");
            Some(false)
        }
    }
}

async fn fetch(client: &Client, user: &UserId) -> matrix_sdk::HttpResult<Presence> {
    let response = client
        .send(get_presence::v3::Request::new(user.to_owned()))
        .await?;
    Ok((&response.presence).into())
}

async fn set(client: &Client, own: &UserId, state: PresenceState) -> matrix_sdk::HttpResult<()> {
    client
        .send(set_presence::v3::Request::new(own.to_owned(), state))
        .await
        .map(|_| ())
}

/// Tell the server we're gone, before signing off. Harmless on servers
/// without presence.
pub async fn sign_off(client: &Client) {
    if let Some(own) = client.user_id()
        && let Err(e) = set(client, own, PresenceState::Offline).await
    {
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
}
