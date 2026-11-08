//! Auto-reply: while you're away, answer each IM's first new message with
//! your away message, like AIM's auto response. DMs only; once per
//! conversation per time away. The reply is an `m.notice`, which clients
//! don't answer, so two people away can't bounce replies back and forth.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use matrix_sdk::{
    Client, Room,
    event_handler::EventHandlerHandle,
    ruma::{
        OwnedRoomId, RoomId,
        events::{
            AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
            room::{
                encrypted::OriginalSyncRoomEncryptedEvent,
                message::{
                    MessageType, OriginalSyncRoomMessageEvent, Relation, RoomMessageEventContent,
                },
            },
        },
    },
};

use tokio::task::JoinSet;

use super::{
    presence::Tracker,
    rooms::{self, Kind},
};

/// What the reply says before the away message.
pub const PREFIX: &str = "Auto-reply: ";

/// A message that arrived, as far as deciding to reply goes.
#[derive(Clone, Debug)]
pub struct Incoming {
    pub from_us: bool,
    /// When the server got it.
    pub sent: SystemTime,
    /// A notice (another auto-reply or a bot) or an edit.
    pub automatic_or_edit: bool,
    pub in_dm: bool,
}

/// Which conversations got a reply, for the time away they got it in.
#[derive(Default)]
pub struct Replied {
    away_since: Option<SystemTime>,
    rooms: HashSet<OwnedRoomId>,
}

impl Replied {
    /// Whether to answer `message` in `room`, given whether auto-reply is on
    /// and when we went away (`None` if we aren't). Answering is recorded,
    /// so a second message gets `false`.
    #[cfg(test)]
    pub fn decide(
        &mut self,
        enabled: bool,
        away_since: Option<SystemTime>,
        room: &OwnedRoomId,
        message: &Incoming,
    ) -> bool {
        self.skip_reason(enabled, away_since, room, message)
            .is_none()
    }

    /// Whether `room` already got a reply in the time away that started
    /// `since`.
    pub fn has_replied(&self, since: SystemTime, room: &RoomId) -> bool {
        self.away_since == Some(since) && self.rooms.contains(room)
    }

    /// [`Replied::decide`], saying why not: `None` means reply (and it's
    /// recorded).
    pub fn skip_reason(
        &mut self,
        enabled: bool,
        away_since: Option<SystemTime>,
        room: &OwnedRoomId,
        message: &Incoming,
    ) -> Option<&'static str> {
        let Some(since) = away_since else {
            return Some("not away");
        };
        // A new time away starts over.
        if self.away_since != Some(since) {
            self.away_since = Some(since);
            self.rooms.clear();
        }
        // The sender's server's clock against ours; with both kept right
        // by NTP, close enough. A message from just before we went away isn't new.
        if !enabled {
            Some("auto-reply is off in Preferences")
        } else if message.from_us {
            Some("it's ours")
        } else if message.automatic_or_edit {
            Some("it's a notice or an edit")
        } else if !message.in_dm {
            Some("not an IM")
        } else if message.sent < since {
            Some("sent before going away")
        } else if !self.rooms.insert(room.clone()) {
            Some("already replied in this conversation")
        } else {
            None
        }
    }
}

/// How long to keep trying to read a message that arrived before its key.
/// Room keys come over a separate connection and usually follow within a
/// second or two.
const DECRYPT_FOR: Duration = Duration::from_secs(60);

/// Most messages waited on for their keys at once. More are skipped: a
/// burst of unreadable messages shouldn't turn into a burst of requests.
const MAX_WAITING: usize = 8;

/// The event handlers answering IMs; [`Watching::stop`] removes them and
/// stops any reply still being worked on.
pub struct Watching {
    handlers: Vec<EventHandlerHandle>,
    replier: Arc<Replier>,
}

impl Watching {
    pub fn stop(self, client: &Client) {
        for handler in self.handlers {
            client.remove_event_handler(handler);
        }
        self.replier.tasks.lock().unwrap().abort_all();
    }
}

/// What deciding and replying needs, shared by both handlers.
struct Replier {
    presence: Arc<Tracker>,
    replied: Mutex<Replied>,
    enabled: Box<dyn Fn() -> bool + Send + Sync>,
    /// Replies being worked on. The SDK waits for each event handler before
    /// syncing on, so the handlers only start these and return.
    tasks: Mutex<JoinSet<()>>,
}

impl Replier {
    /// Run `work` in the background, unless `limit` tasks are already
    /// running.
    fn spawn(&self, limit: usize, work: impl Future<Output = ()> + Send + 'static) -> bool {
        let mut tasks = self.tasks.lock().unwrap();
        while tasks.try_join_next().is_some() {}
        if tasks.len() >= limit {
            return false;
        }
        tasks.spawn(work);
        true
    }

    /// Whether `room` is an IM that auto-reply may answer: a DM by AIMtrix's
    /// rules, and really just two people. A room in `m.direct` can grow, and
    /// the away message shouldn't go to everyone in it.
    async fn is_im(room: &Room) -> bool {
        room.active_members_count() <= 2 && rooms::kind_of(room).await == Kind::Dm
    }

    /// Answer `event` in `room` if it calls for it.
    async fn consider(&self, room: &Room, event: &OriginalSyncRoomMessageEvent) {
        let away = self.presence.away();
        let message = Incoming {
            from_us: room.own_user_id() == event.sender,
            // A timestamp too far off to be a time (Windows' clock can't
            // reach it) is as good as old.
            sent: UNIX_EPOCH
                .checked_add(Duration::from_millis(event.origin_server_ts.0.into()))
                .unwrap_or(UNIX_EPOCH),
            automatic_or_edit: matches!(event.content.msgtype, MessageType::Notice(_))
                || matches!(event.content.relates_to, Some(Relation::Replacement(_))),
            in_dm: Self::is_im(room).await,
        };
        let room_id = room.room_id().to_owned();
        let skip = if room.client().is_user_ignored(&event.sender).await {
            Some("the sender is blocked")
        } else {
            self.replied.lock().unwrap().skip_reason(
                (self.enabled)(),
                away.as_ref().map(|(_, since)| *since),
                &room_id,
                &message,
            )
        };
        if let Some(reason) = skip {
            // Only IMs from others while it's on and we're away, so normal
            // use doesn't fill the log.
            let on_and_away = away.is_some() && (self.enabled)();
            if on_and_away && message.in_dm && !message.from_us {
                eprintln!("Auto-reply: not replying in {room_id}: {reason}");
            }
            return;
        }
        let Some((away_message, _)) = away else {
            return;
        };
        eprintln!("Auto-reply: replying in {room_id}");
        let reply = RoomMessageEventContent::notice_plain(format!("{PREFIX}{away_message}"));
        if let Err(e) = room.send(reply).await {
            eprintln!("Couldn't send an auto-reply: {e}");
        }
    }

    /// An encrypted message we couldn't read yet: if it might call for a
    /// reply, keep trying to decrypt it, then consider it.
    async fn consider_later(&self, room: &Room, event: &OriginalSyncRoomEncryptedEvent) {
        if room.own_user_id() == event.sender || !Self::is_im(room).await {
            return;
        }
        let Some((_, since)) = self.presence.away() else {
            return;
        };
        if !(self.enabled)()
            || self
                .replied
                .lock()
                .unwrap()
                .has_replied(since, room.room_id())
            || room.client().is_user_ignored(&event.sender).await
        {
            return;
        }
        eprintln!(
            "Auto-reply: waiting for the key to a message in {}",
            room.room_id()
        );
        // Asking decrypts with any key that has arrived since. Ask again
        // after 1, 2, 4… seconds, so a key that never comes costs a handful
        // of requests, not one a second.
        let deadline = tokio::time::Instant::now() + DECRYPT_FOR;
        let mut pause = Duration::from_secs(1);
        while tokio::time::Instant::now() + pause <= deadline {
            tokio::time::sleep(pause).await;
            pause *= 2;
            let Ok(decrypted) = room.event(&event.event_id, None).await else {
                continue;
            };
            if decrypted.encryption_info().is_none() {
                continue;
            }
            if let Ok(AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
                SyncMessageLikeEvent::Original(message),
            ))) = decrypted.raw().deserialize()
            {
                self.consider(room, &message).await;
            }
            return;
        }
        eprintln!(
            "Auto-reply: gave up on a message in {} that never decrypted",
            room.room_id()
        );
    }
}

/// Answer IMs while away, until [`Watching::stop`]. `enabled` says whether
/// auto-reply is switched on in Preferences.
pub fn watch(
    client: &Client,
    presence: Arc<Tracker>,
    enabled: impl Fn() -> bool + Send + Sync + 'static,
) -> Watching {
    let replier = Arc::new(Replier {
        presence,
        replied: Mutex::default(),
        enabled: Box::new(enabled),
        tasks: Mutex::default(),
    });
    let readable = client.add_event_handler({
        let replier = replier.clone();
        move |event: OriginalSyncRoomMessageEvent, room: Room| {
            let worker = replier.clone();
            replier.spawn(
                usize::MAX,
                async move { worker.consider(&room, &event).await },
            );
            async {}
        }
    });
    // Sync hands over messages it couldn't decrypt as `m.room.encrypted`,
    // and doesn't run handlers again once they're decrypted.
    let unreadable = client.add_event_handler({
        let replier = replier.clone();
        move |event: OriginalSyncRoomEncryptedEvent, room: Room| {
            let worker = replier.clone();
            let room_id = room.room_id().to_owned();
            let waiting = replier.spawn(MAX_WAITING, async move {
                worker.consider_later(&room, &event).await
            });
            if !waiting {
                eprintln!("Auto-reply: too many unreadable messages, skipping one in {room_id}");
            }
            async {}
        }
    });
    Watching {
        handlers: vec![readable, unreadable],
        replier,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message() -> Incoming {
        Incoming {
            from_us: false,
            sent: SystemTime::now(),
            automatic_or_edit: false,
            in_dm: true,
        }
    }

    fn room(id: &str) -> OwnedRoomId {
        OwnedRoomId::try_from(id).unwrap()
    }

    #[test]
    fn replies_once_per_conversation() {
        let mut replied = Replied::default();
        let since = Some(SystemTime::now() - Duration::from_secs(60));
        let (a, b) = (room("!a:localhost"), room("!b:localhost"));
        assert!(replied.decide(true, since, &a, &message()));
        assert!(!replied.decide(true, since, &a, &message()));
        assert!(replied.decide(true, since, &b, &message()));
    }

    #[test]
    fn coming_back_and_going_away_again_replies_again() {
        let mut replied = Replied::default();
        let a = room("!a:localhost");
        let first = SystemTime::now() - Duration::from_secs(600);
        assert!(replied.decide(true, Some(first), &a, &message()));
        assert!(!replied.decide(true, None, &a, &message()));
        let second = SystemTime::now() - Duration::from_secs(60);
        assert!(replied.decide(true, Some(second), &a, &message()));
    }

    #[test]
    fn only_new_messages_from_others_in_ims_while_on_and_away() {
        let mut replied = Replied::default();
        let a = room("!a:localhost");
        let since = SystemTime::now() - Duration::from_secs(60);
        let cases = [
            ("switched off", false, Some(since), message()),
            ("not away", true, None, message()),
            (
                "ours",
                true,
                Some(since),
                Incoming {
                    from_us: true,
                    ..message()
                },
            ),
            (
                "a notice or edit",
                true,
                Some(since),
                Incoming {
                    automatic_or_edit: true,
                    ..message()
                },
            ),
            (
                "a Chat Room",
                true,
                Some(since),
                Incoming {
                    in_dm: false,
                    ..message()
                },
            ),
            (
                "from before we went away",
                true,
                Some(since),
                Incoming {
                    sent: since - Duration::from_secs(30),
                    ..message()
                },
            ),
        ];
        for (what, enabled, away, msg) in cases {
            assert!(
                !replied.decide(enabled, away, &a, &msg),
                "replied to {what}"
            );
        }
        // None of those used up the conversation's reply.
        assert!(replied.decide(true, Some(since), &a, &message()));
    }

    #[test]
    fn messages_from_just_before_going_away_get_no_reply() {
        let mut replied = Replied::default();
        let since = SystemTime::now();
        let just_before = Incoming {
            sent: since - Duration::from_secs(2),
            ..message()
        };
        assert!(!replied.decide(true, Some(since), &room("!a:localhost"), &just_before));
    }
}
