//! Who's in a room, for the Chat Room's member list.
//!
//! [`watch`] sends the whole list again whenever a member event arrives
//! (someone joins, leaves or changes their name or avatar) and when a
//! Buddy Icon finishes downloading. Lists are small, so there's no diffing.

use std::sync::Arc;

use matrix_sdk::{
    Client, Room, RoomMemberships, RoomState, event_handler::EventHandlerHandle, room::RoomMember,
    ruma::RoomId, ruma::events::room::member::SyncRoomMemberEvent,
};
use serde::Serialize;
use tauri::async_runtime::{JoinHandle, spawn};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

use super::media::Icons;

/// Someone in the room.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub user_id: String,
    /// Display name, or the localpart if there's none.
    pub name: String,
    /// You.
    pub own: bool,
    /// Buddy Icon as a `data:` URL. `None` means show the default icon.
    pub icon: Option<String>,
}

/// Follow who's in `room_id`. Returns the current members; every later
/// change goes to `on_change` as the whole list, until the [`Watching`] is
/// dropped.
pub async fn watch(
    client: &Client,
    room_id: &str,
    on_change: impl Fn(Vec<Member>) + Send + 'static,
) -> Result<(Vec<Member>, Watching), String> {
    let room_id = RoomId::parse(room_id).map_err(|e| format!("Bad room ID: {e}"))?;
    let room = match client.get_room(&room_id) {
        Some(room) if room.state() == RoomState::Joined => room,
        _ => return Err("You're not in that room.".to_owned()),
    };

    let (changed, mut changes) = unbounded_channel();
    let handler = client.add_room_event_handler(&room_id, move |_: SyncRoomMemberEvent| {
        let changed = changed.clone();
        async move {
            let _ = changed.send(());
        }
    });
    let (ready, mut icons_ready) = unbounded_channel();
    let icons = Icons::new(client.clone(), ready);

    let first = match list(&room, &icons).await {
        Ok(first) => first,
        Err(e) => {
            client.remove_event_handler(handler);
            return Err(e);
        }
    };
    let task = spawn(async move {
        loop {
            tokio::select! {
                Some(()) = changes.recv() => drain(&mut changes),
                Some(()) = icons_ready.recv() => drain(&mut icons_ready),
                else => break,
            }
            match list(&room, &icons).await {
                Ok(members) => on_change(members),
                Err(e) => eprintln!("{e}"),
            }
        }
    });
    Ok((
        first,
        Watching {
            client: client.clone(),
            handler: Some(handler),
            task,
        },
    ))
}

/// Skip changes that queued up meanwhile: one new list covers them all.
fn drain(rx: &mut UnboundedReceiver<()>) {
    while rx.try_recv().is_ok() {}
}

/// The joined members, sorted by name. The first call loads them all from
/// the server; sync keeps them current after that.
async fn list(room: &Room, icons: &Arc<Icons>) -> Result<Vec<Member>, String> {
    let own = room.own_user_id().to_owned();
    let mut members: Vec<Member> = room
        .members(RoomMemberships::JOIN)
        .await
        .map_err(|e| format!("Couldn't load who's in {}: {e}", room.room_id()))?
        .iter()
        .map(|m| member(m, *m.user_id() == own, icons))
        .collect();
    sort(&mut members);
    Ok(members)
}

fn member(m: &RoomMember, own: bool, icons: &Arc<Icons>) -> Member {
    Member {
        user_id: m.user_id().to_string(),
        name: m.name().to_owned(),
        own,
        icon: m.avatar_url().and_then(|mxc| icons.get(&mxc.to_owned())),
    }
}

/// By name, ignoring case; same names by user ID so the order is stable.
fn sort(members: &mut [Member]) {
    members.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.user_id.cmp(&b.user_id))
    });
}

/// A running watch. Dropping it stops it.
pub struct Watching {
    client: Client,
    handler: Option<EventHandlerHandle>,
    task: JoinHandle<()>,
}

impl Drop for Watching {
    fn drop(&mut self) {
        if let Some(handler) = self.handler.take() {
            self.client.remove_event_handler(handler);
        }
        self.task.abort();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(user_id: &str, name: &str) -> Member {
        Member {
            user_id: user_id.to_owned(),
            name: name.to_owned(),
            own: false,
            icon: None,
        }
    }

    #[test]
    fn members_sort_by_name_ignoring_case() {
        let mut members = vec![
            named("@c:x", "carol"),
            named("@b2:x", "Bob"),
            named("@a:x", "Alice"),
            named("@b1:x", "Bob"),
        ];
        sort(&mut members);
        let ids: Vec<_> = members.iter().map(|m| m.user_id.as_str()).collect();
        assert_eq!(ids, ["@a:x", "@b1:x", "@b2:x", "@c:x"]);
    }

    #[test]
    fn members_serialize_for_the_window() {
        assert_eq!(
            serde_json::to_value(named("@a:x", "Alice")).unwrap(),
            serde_json::json!({"userId": "@a:x", "name": "Alice", "own": false, "icon": null})
        );
    }
}
