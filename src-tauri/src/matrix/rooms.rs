//! Turns Matrix rooms into Buddy List entries: DM or group, and which group
//! of the list ("Favorites", "Buddies", "Chats", "Invitations") they go in.
//! On servers with presence, "Buddies" splits into "Online" and "Offline".
//!
//! The rules are pure functions over [`RoomFacts`], so they're unit-tested
//! without a server. [`watch`] applies them to the live room list.

use std::sync::Arc;

use eyeball_im::Vector;
use futures_util::StreamExt;
use matrix_sdk::{
    Client, Room, RoomState,
    ruma::{OwnedMxcUri, OwnedUserId, RoomId},
};
use matrix_sdk_ui::room_list_service::{
    RoomListItem, RoomListService, filters::new_filter_non_left,
};
use serde::Serialize;
use tauri::async_runtime::{JoinHandle, spawn};
use tokio::sync::mpsc::unbounded_channel;

use super::{
    media::{self, Icons},
    presence::{Presence, Status, Tracker},
};

/// Room tag that forces a room to be shown as a Chat Room, even if it looks
/// like a DM.
pub const CHAT_TAG: &str = "aimtrix.chat";

/// Our membership in the room.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Membership {
    Joined,
    Invited,
    Left,
}

/// What classification needs to know about a room.
#[derive(Clone, Debug)]
pub struct RoomFacts {
    pub membership: Membership,
    /// Listed in our `m.direct` account data. Can be stale either way.
    pub in_m_direct: bool,
    /// Joined plus invited members, us included.
    pub member_count: u64,
    /// Has the `m.favourite` tag.
    pub favourite: bool,
    /// Has the [`CHAT_TAG`] tag.
    pub chat_tag: bool,
    /// The other person's presence, for a DM. `None` when the server has no
    /// presence.
    pub presence: Option<Presence>,
}

/// Which window a room opens in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// One-to-one: opens an IM window.
    Dm,
    /// Several people: opens a Chat Room window.
    Group,
}

/// A group of the Buddy List, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Section {
    Favorites,
    /// DMs, when the server has no presence.
    Buddies,
    /// DMs whose other person is online or away.
    Online,
    Chats,
    Invitations,
    /// DMs whose other person is offline. Last, as in AIM.
    Offline,
}

/// A room is a DM if it is in `m.direct` or has at most two members, unless
/// tagged [`CHAT_TAG`].
pub fn kind(room: &RoomFacts) -> Kind {
    if room.chat_tag {
        Kind::Group
    } else if room.in_m_direct || room.member_count <= 2 {
        Kind::Dm
    } else {
        Kind::Group
    }
}

/// The Buddy List group a room goes in, or `None` for rooms we've left.
pub fn section(room: &RoomFacts) -> Option<Section> {
    match room.membership {
        Membership::Left => None,
        Membership::Invited => Some(Section::Invitations),
        Membership::Joined if room.favourite => Some(Section::Favorites),
        Membership::Joined => Some(match (kind(room), room.presence) {
            (Kind::Group, _) => Section::Chats,
            (Kind::Dm, None) => Section::Buddies,
            (Kind::Dm, Some(Presence::Offline)) => Section::Offline,
            (Kind::Dm, Some(Presence::Online | Presence::Away)) => Section::Online,
        }),
    }
}

/// One room as the Buddy List shows it.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Buddy {
    pub room_id: String,
    pub name: String,
    pub kind: Kind,
    pub section: Section,
    /// Has messages we haven't read, or was marked unread.
    pub unread: bool,
    /// Buddy Icon as a `data:` URL. `None` means show the default icon.
    pub icon: Option<String>,
    /// The other person's presence, for a joined DM on a server with
    /// presence.
    pub presence: Option<Presence>,
    /// The other person's status message, shown as their away message.
    pub away_message: Option<String>,
}

/// Most rooms the list holds. Far more than anyone keeps on a Buddy List.
const MAX_ROOMS: usize = 10_000;

/// Watch the room list in the background. `on_update` gets the whole list,
/// most recently active first, every time it changes, an icon arrives or a
/// buddy's presence (per `presence`) changes.
pub fn watch(
    service: Arc<RoomListService>,
    presence: Arc<Tracker>,
    on_update: impl Fn(Vec<Buddy>) + Send + 'static,
) -> JoinHandle<()> {
    spawn(async move {
        let list = match service.all_rooms().await {
            Ok(list) => list,
            Err(e) => {
                eprintln!("Couldn't get the room list: {e}");
                return;
            }
        };
        let (diffs, controller) = list.entries_with_dynamic_adapters(MAX_ROOMS);
        controller.set_filter(Box::new(new_filter_non_left()));
        let mut diffs = std::pin::pin!(diffs);
        let (icon_ready, mut icons_ready) = unbounded_channel();
        let icons = Icons::new(service.client().clone(), icon_ready);
        let mut presence_changed = presence.subscribe();

        let mut rooms = Vector::new();
        loop {
            tokio::select! {
                Some(batch) = diffs.next() => {
                    for diff in batch {
                        diff.apply(&mut rooms);
                    }
                }
                Some(()) = icons_ready.recv() => {}
                Ok(()) = presence_changed.changed() => {}
                else => break,
            }
            let mut buddies = Vec::with_capacity(rooms.len());
            for room in &rooms {
                if let Some(buddy) = buddy(room, &icons, &presence).await {
                    buddies.push(buddy);
                }
            }
            on_update(buddies);
        }
    })
}

async fn buddy(room: &RoomListItem, icons: &Arc<Icons>, presence: &Tracker) -> Option<Buddy> {
    let membership = match room.state() {
        RoomState::Joined => Membership::Joined,
        RoomState::Invited => Membership::Invited,
        RoomState::Left | RoomState::Knocked | RoomState::Banned => Membership::Left,
    };
    // A store error just means we can't see the tag; show the room anyway.
    let chat_tag = room
        .tags()
        .await
        .ok()
        .flatten()
        .is_some_and(|tags| tags.keys().any(|tag| tag.as_ref() == CHAT_TAG));
    let mut facts = RoomFacts {
        membership,
        // Also checks the invite's `is_direct` flag, which `m.direct` lacks
        // until we join.
        in_m_direct: room.is_direct().await.unwrap_or(false),
        member_count: room.active_members_count(),
        favourite: room.is_favourite(),
        chat_tag,
        presence: None,
    };
    let kind = kind(&facts);
    let mut away_message = None;
    if kind == Kind::Dm && membership == Membership::Joined {
        let status = match other_person(room).await {
            Some(user) => presence.get(&user),
            // A DM whose other person left has no one to show.
            None => presence.supported().filter(|&s| s).map(|_| Status {
                presence: Presence::Offline,
                message: None,
            }),
        };
        if let Some(status) = status {
            facts.presence = Some(status.presence);
            away_message = status.message;
        }
    }
    let avatar = avatar(room, kind).await;
    Some(Buddy {
        room_id: room.room_id().to_string(),
        name: room
            .cached_display_name()
            .map(|name| name.to_string())
            .unwrap_or_else(|| room.room_id().to_string()),
        kind,
        section: section(&facts)?,
        unread: room.num_unread_messages() > 0 || room.is_marked_unread(),
        icon: avatar.and_then(|mxc| icons.get(&mxc)),
        presence: facts.presence,
        away_message,
    })
}

/// The other person in a DM: from `m.direct` if it says, otherwise the
/// room's first hero (heroes leave us out).
async fn other_person(room: &Room) -> Option<OwnedUserId> {
    let own = room.own_user_id();
    room.direct_targets()
        .iter()
        .filter_map(|target| target.as_user_id())
        .find(|&user| user != own)
        .map(ToOwned::to_owned)
        .or(room
            .heroes()
            .await
            .into_iter()
            .next()
            .map(|hero| hero.user_id))
}

/// The avatar a room shows as its Buddy Icon. A DM without its own avatar
/// shows the other person's.
async fn avatar(room: &Room, kind: Kind) -> Option<OwnedMxcUri> {
    match (room.avatar_url(), kind) {
        (Some(url), _) => Some(url),
        (None, Kind::Dm) => room
            .heroes()
            .await
            .into_iter()
            .find_map(|hero| hero.avatar_url),
        (None, Kind::Group) => None,
    }
}

/// A Buddy List room's icon at `size` px, as a `data:` URL, or `None` for
/// the default icon.
pub async fn icon(client: &Client, buddy: &Buddy, size: u32) -> Result<Option<String>, String> {
    let room_id = RoomId::parse(&buddy.room_id).map_err(|e| format!("Bad room ID: {e}"))?;
    let room = client
        .get_room(&room_id)
        .ok_or_else(|| "That room isn't on your Buddy List.".to_owned())?;
    match avatar(&room, buddy.kind).await {
        Some(mxc) => media::icon(client, &mxc, size)
            .await
            .map(Some)
            .map_err(|e| format!("Couldn't fetch the Buddy Icon: {e}")),
        None => Ok(None),
    }
}

/// Accept an invite. The room moves out of Invitations on the next update;
/// the SDK adds DM invites to `m.direct`.
pub async fn accept(client: &Client, room_id: &str) -> Result<(), String> {
    invite(client, room_id)?
        .join()
        .await
        .map_err(|e| format!("Couldn't accept the invitation: {e}"))
}

/// Decline an invite.
pub async fn decline(client: &Client, room_id: &str) -> Result<(), String> {
    invite(client, room_id)?
        .leave()
        .await
        .map_err(|e| format!("Couldn't decline the invitation: {e}"))
}

fn invite(client: &Client, room_id: &str) -> Result<Room, String> {
    let room_id = RoomId::parse(room_id).map_err(|e| format!("Bad room ID: {e}"))?;
    match client.get_room(&room_id) {
        Some(room) if room.state() == RoomState::Invited => Ok(room),
        _ => Err("That invitation is no longer open.".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(member_count: u64) -> RoomFacts {
        RoomFacts {
            membership: Membership::Joined,
            in_m_direct: false,
            member_count,
            favourite: false,
            chat_tag: false,
            presence: None,
        }
    }

    fn dm() -> RoomFacts {
        RoomFacts {
            in_m_direct: true,
            ..room(2)
        }
    }

    #[test]
    fn dm_goes_in_buddies() {
        assert_eq!(kind(&dm()), Kind::Dm);
        assert_eq!(section(&dm()), Some(Section::Buddies));
    }

    #[test]
    fn with_presence_dms_split_into_online_and_offline() {
        let with = |presence| RoomFacts {
            presence: Some(presence),
            ..dm()
        };
        assert_eq!(section(&with(Presence::Online)), Some(Section::Online));
        assert_eq!(section(&with(Presence::Away)), Some(Section::Online));
        assert_eq!(section(&with(Presence::Offline)), Some(Section::Offline));
    }

    #[test]
    fn presence_leaves_favorites_where_they_are() {
        let offline_favourite = RoomFacts {
            presence: Some(Presence::Offline),
            favourite: true,
            ..dm()
        };
        assert_eq!(section(&offline_favourite), Some(Section::Favorites));
    }

    #[test]
    fn group_goes_in_chats() {
        assert_eq!(kind(&room(3)), Kind::Group);
        assert_eq!(section(&room(3)), Some(Section::Chats));
    }

    #[test]
    fn two_person_room_missing_from_m_direct_is_a_dm() {
        // Stale m.direct: another client made the DM without listing it.
        assert_eq!(kind(&room(2)), Kind::Dm);
        assert_eq!(section(&room(2)), Some(Section::Buddies));
    }

    #[test]
    fn room_whose_other_member_left_is_still_a_dm() {
        assert_eq!(kind(&room(1)), Kind::Dm);
        assert_eq!(
            kind(&RoomFacts {
                in_m_direct: true,
                ..room(1)
            }),
            Kind::Dm
        );
    }

    #[test]
    fn room_in_m_direct_that_grew_is_still_a_dm() {
        // Stale m.direct: people were added after it was listed. The rule
        // keeps it a DM; the chat tag is the way out.
        let grown = RoomFacts {
            in_m_direct: true,
            ..room(5)
        };
        assert_eq!(kind(&grown), Kind::Dm);
        assert_eq!(section(&grown), Some(Section::Buddies));
    }

    #[test]
    fn chat_tag_forces_a_group() {
        for facts in [
            room(1),
            room(2),
            dm(),
            RoomFacts {
                in_m_direct: true,
                ..room(5)
            },
        ] {
            let tagged = RoomFacts {
                chat_tag: true,
                ..facts
            };
            assert_eq!(kind(&tagged), Kind::Group, "{tagged:?}");
            assert_eq!(section(&tagged), Some(Section::Chats), "{tagged:?}");
        }
    }

    #[test]
    fn chat_tag_on_a_group_changes_nothing() {
        let tagged = RoomFacts {
            chat_tag: true,
            ..room(4)
        };
        assert_eq!(kind(&tagged), Kind::Group);
        assert_eq!(section(&tagged), Some(Section::Chats));
    }

    #[test]
    fn favourites_go_in_favorites_whatever_their_kind() {
        let fav_dm = RoomFacts {
            favourite: true,
            ..dm()
        };
        let fav_group = RoomFacts {
            favourite: true,
            ..room(6)
        };
        assert_eq!(section(&fav_dm), Some(Section::Favorites));
        assert_eq!(section(&fav_group), Some(Section::Favorites));
        // Kind still decides the window.
        assert_eq!(kind(&fav_dm), Kind::Dm);
        assert_eq!(kind(&fav_group), Kind::Group);
    }

    #[test]
    fn invites_go_in_invitations() {
        for facts in [
            dm(),
            room(6),
            RoomFacts {
                favourite: true,
                ..room(6)
            },
        ] {
            let invite = RoomFacts {
                membership: Membership::Invited,
                ..facts
            };
            assert_eq!(section(&invite), Some(Section::Invitations), "{invite:?}");
        }
    }

    #[test]
    fn invites_keep_their_kind() {
        let invite = RoomFacts {
            membership: Membership::Invited,
            ..dm()
        };
        assert_eq!(kind(&invite), Kind::Dm);
        let invite = RoomFacts {
            membership: Membership::Invited,
            ..room(6)
        };
        assert_eq!(kind(&invite), Kind::Group);
    }

    #[test]
    fn left_rooms_are_not_listed() {
        for facts in [
            dm(),
            room(6),
            RoomFacts {
                favourite: true,
                ..room(6)
            },
        ] {
            let left = RoomFacts {
                membership: Membership::Left,
                ..facts
            };
            assert_eq!(section(&left), None, "{left:?}");
        }
    }

    #[test]
    fn sections_sort_in_display_order() {
        let mut sections = [
            Section::Offline,
            Section::Invitations,
            Section::Chats,
            Section::Online,
            Section::Favorites,
            Section::Buddies,
        ];
        sections.sort();
        assert_eq!(
            sections,
            [
                Section::Favorites,
                Section::Buddies,
                Section::Online,
                Section::Chats,
                Section::Invitations,
                Section::Offline,
            ]
        );
    }
}
