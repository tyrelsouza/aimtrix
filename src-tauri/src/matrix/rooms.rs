//! Turns Matrix rooms into Buddy List entries: DM or group, and which group
//! of the list ("Favorites", "Buddies", "Chats", "Invitations") they go in.
//!
//! Pure functions over [`RoomFacts`], so the rules are unit-tested without a
//! server. Step 2.2 fills `RoomFacts` from the room list.

// Nothing calls this until step 2.2 wires up the room list.
#![allow(dead_code)]

use serde::Serialize;

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
    Buddies,
    Chats,
    Invitations,
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
        Membership::Joined => Some(match kind(room) {
            Kind::Dm => Section::Buddies,
            Kind::Group => Section::Chats,
        }),
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
            Section::Invitations,
            Section::Chats,
            Section::Favorites,
            Section::Buddies,
        ];
        sections.sort();
        assert_eq!(
            sections,
            [
                Section::Favorites,
                Section::Buddies,
                Section::Chats,
                Section::Invitations
            ]
        );
    }
}
