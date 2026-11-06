//! A room's conversation, as lines for its IM or Chat Room window.
//!
//! [`watch`] follows a room's [`Timeline`] and turns each change into a
//! [`Diff`] of [`Line`]s. Every timeline item maps to exactly one line, so
//! the window can apply the diffs by index. [`Watching::send`] sends
//! through the same timeline, so a sent message shows up at once. The watch
//! also reports who is typing.

use eyeball_im::VectorDiff;
use futures_util::StreamExt;
use matrix_sdk::{
    Client, Room, RoomState,
    media::{MediaFormat, MediaRequestParameters, MediaThumbnailSettings},
    ruma::{
        EventId, OwnedEventId, OwnedUserId, RoomId, UserId,
        api::client::receipt::create_receipt::v3::ReceiptType,
        events::{
            StateEventContentChange,
            room::{
                MediaSource,
                message::{MessageFormat, MessageType, RoomMessageEventContent},
            },
        },
    },
};
use matrix_sdk_crypto::types::events::UtdCause;
use matrix_sdk_ui::timeline::{
    AnyOtherStateEventContentChange, AttachmentSource, EncryptedMessage, EventSendState,
    MembershipChange, MsgLikeKind, Profile, RoomExt, Timeline, TimelineDetails, TimelineItem,
    TimelineItemContent, VirtualTimelineItem,
};
use serde::Serialize;
use std::{collections::HashSet, future::IntoFuture, path::PathBuf, sync::Arc};
use tauri::async_runtime::{JoinHandle, spawn, spawn_blocking};

use super::{html, media, upload};
use tokio::sync::broadcast::error::RecvError;

/// History to load when the window opens, and each time you scroll up.
const PAGE: u16 = 50;

/// One line of the conversation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub enum Line {
    /// "Name: text", or an image or file.
    Message {
        id: String,
        /// `None` until the server has it (our own, while sending).
        event_id: Option<String>,
        sender: String,
        name: String,
        /// Sent by us.
        own: bool,
        /// Plain text, always there.
        body: String,
        /// Cleaned formatted text, if the message has any.
        html: Option<String>,
        /// `/me`: shown as "* Name text".
        emote: bool,
        /// Changed after sending; `body` and `html` are the latest version.
        edited: bool,
        /// An image or file. `body` is then its caption, or its name.
        media: Option<Media>,
        /// The message this one answers.
        reply: Option<Reply>,
        /// Ours, and the server didn't take it.
        failed: bool,
        /// Milliseconds since the Unix epoch.
        timestamp: u64,
    },
    /// A message that was deleted: "Name: (message deleted)".
    Deleted {
        id: String,
        name: String,
        own: bool,
        timestamp: u64,
    },
    /// An encrypted message we can't read (yet): "Name: (encrypted
    /// message…)". It turns into a [`Line::Message`] by itself if the key
    /// arrives later.
    Undecryptable {
        id: String,
        event_id: Option<String>,
        name: String,
        own: bool,
        reason: UndecryptableReason,
        timestamp: u64,
    },
    /// Someone entered or left the room, or was invited, removed or banned:
    /// "Name has entered the room."
    Membership {
        id: String,
        /// Whose membership changed (not who changed it).
        name: String,
        change: MemberChange,
        timestamp: u64,
    },
    /// The topic was set or removed.
    Topic {
        id: String,
        /// Who changed it.
        name: String,
        /// `None` when it was removed.
        topic: Option<String>,
        timestamp: u64,
    },
    /// The room's first event is loaded: there's no more history.
    Start { id: String },
    /// Not shown yet: other message types, other state events, date dividers…
    Other { id: String },
}

/// Why an encrypted message can't be read, as far as we can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UndecryptableReason {
    /// The key may still come: from the sender, a backup or another device.
    Waiting,
    /// Sent before this device signed on, and there's no key backup to get
    /// it from (or this device can't use it until it's verified).
    BeforeSignOn,
    /// Sent before you joined the room.
    BeforeJoined,
    /// The sender chose not to share the key with this device.
    Withheld,
    /// Sent from a device or person we can't trust, so it was refused.
    Untrusted,
}

impl UndecryptableReason {
    fn from_sdk(cause: UtdCause) -> Self {
        use UtdCause as C;
        match cause {
            C::SentBeforeWeJoined => Self::BeforeJoined,
            C::HistoricalMessageAndBackupIsDisabled | C::HistoricalMessageAndDeviceIsUnverified => {
                Self::BeforeSignOn
            }
            C::WithheldForUnverifiedOrInsecureDevice | C::WithheldBySender => Self::Withheld,
            C::VerificationViolation | C::UnsignedDevice | C::UnknownDevice => Self::Untrusted,
            _ => Self::Waiting,
        }
    }
}

/// A change of membership worth a line in the conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemberChange {
    Joined,
    Left,
    Invited,
    Kicked,
    Banned,
}

impl MemberChange {
    /// `None` for changes not worth a line, like a declined invite or a
    /// knock.
    fn from_sdk(change: MembershipChange) -> Option<Self> {
        use MembershipChange as C;
        match change {
            C::Joined | C::InvitationAccepted | C::KnockAccepted => Some(Self::Joined),
            C::Left => Some(Self::Left),
            C::Invited => Some(Self::Invited),
            C::Kicked => Some(Self::Kicked),
            C::Banned | C::KickedAndBanned => Some(Self::Banned),
            _ => None,
        }
    }
}

/// What kind of attachment a message has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    File,
}

/// An attachment. Images show a thumbnail from [`Watching::thumbnail`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Media {
    pub kind: MediaKind,
    /// The file name, e.g. `photo.png`.
    pub name: String,
    /// In bytes, if the sender said.
    pub size: Option<u64>,
    /// In pixels, for images and videos, if the sender said.
    pub width: Option<u64>,
    pub height: Option<u64>,
}

/// A short preview of the message a reply answers.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    /// `None` until the message is loaded.
    pub name: Option<String>,
    pub preview: Option<String>,
}

/// Longest reply preview, in characters.
const PREVIEW_LENGTH: usize = 80;

/// Thumbnail size the server is asked for; shown at up to half that, so it
/// stays sharp on high-DPI screens.
const THUMBNAIL_WIDTH: u32 = 480;
const THUMBNAIL_HEIGHT: u32 = 360;

/// Biggest image shown in full when it has no thumbnail (only encrypted
/// images, which the server can't shrink): 10 MB.
const MAX_FULL_IMAGE: u64 = 10_000_000;

/// A change to the lines, mirroring [`VectorDiff`].
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum Diff {
    Append { values: Vec<Line> },
    Clear,
    PushFront { value: Line },
    PushBack { value: Line },
    PopFront,
    PopBack,
    Insert { index: usize, value: Line },
    Set { index: usize, value: Line },
    Remove { index: usize },
    Truncate { length: usize },
    Reset { values: Vec<Line> },
}

/// Follow a joined room's conversation. Returns the current lines; every
/// later change goes to `on_diff`, until the [`Watching`] is dropped. Older
/// history arrives as diffs too.
pub async fn watch(
    client: &Client,
    room_id: &str,
    on_diff: impl Fn(Diff) + Send + 'static,
    on_typing: impl Fn(Vec<String>) + Send + 'static,
) -> Result<(Vec<Line>, Watching), String> {
    let room_id = RoomId::parse(room_id).map_err(|e| format!("Bad room ID: {e}"))?;
    let room = match client.get_room(&room_id) {
        Some(room) if room.state() == RoomState::Joined => room,
        _ => return Err("You're not in that room.".to_owned()),
    };
    let timeline = room
        .timeline()
        .await
        .map_err(|e| format!("Couldn't open the conversation: {e}"))?;
    let timeline = Arc::new(timeline);
    let (items, mut diffs) = timeline.subscribe().await;
    let lines = items.iter().map(|item| line(item)).collect();
    // Replies whose message we've asked the SDK to load.
    let mut replies_asked = HashSet::new();
    for item in &items {
        load_replied_to(&timeline, item, &mut replies_asked);
    }
    // Leaves out our own typing. The guard keeps the subscription alive.
    let (typing_guard, mut typing) = room.subscribe_to_typing_notifications();

    let task = spawn({
        let timeline = timeline.clone();
        async move {
            let _typing_guard = typing_guard;
            // Both send their changes through `diffs`, so run them alongside.
            // Sync loads members lazily, so without fetching them senders can
            // lack display names.
            let setup = async {
                let (history, ()) =
                    tokio::join!(timeline.paginate_backwards(PAGE), timeline.fetch_members());
                if let Err(e) = history {
                    eprintln!(
                        "Couldn't load history for {}: {e}",
                        timeline.room().room_id()
                    );
                }
            };
            let mut setup = std::pin::pin!(setup);
            let mut setting_up = true;
            // The SDK empties every conversation when you block or unblock
            // someone; this loads a page of history back in.
            let mut refill: Option<
                std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
            > = None;
            loop {
                tokio::select! {
                    () = &mut setup, if setting_up => setting_up = false,
                    () = async { refill.as_mut().unwrap().await }, if refill.is_some() => {
                        refill = None;
                    }
                    batch = diffs.next() => match batch {
                        Some(batch) => {
                            if batch.iter().any(|c| matches!(c, VectorDiff::Clear)) && !setting_up {
                                let timeline = timeline.clone();
                                refill = Some(Box::pin(async move {
                                    if let Err(e) = timeline.paginate_backwards(PAGE).await {
                                        eprintln!("Couldn't reload history: {e}");
                                    }
                                }));
                            }
                            for change in batch {
                                for item in diff_items(&change) {
                                    load_replied_to(&timeline, item, &mut replies_asked);
                                }
                                on_diff(diff(change));
                            }
                        }
                        None => break,
                    },
                    users = typing.recv() => match users {
                        Ok(users) => {
                            // Named first, so `on_typing` isn't held across the await.
                            let names = names(timeline.room(), &users).await;
                            on_typing(names);
                        }
                        // Missed some; the next notice has the whole list.
                        Err(RecvError::Lagged(_)) => {}
                        Err(RecvError::Closed) => break,
                    },
                }
            }
        }
    });
    Ok((lines, Watching { timeline, task }))
}

/// A room's conversation being followed. Dropping it stops the diffs.
pub struct Watching {
    timeline: Arc<Timeline>,
    task: JoinHandle<()>,
}

/// Display names of `users` in `room`, for "Name is typing…".
async fn names(room: &Room, users: &[OwnedUserId]) -> Vec<String> {
    let mut names = Vec::with_capacity(users.len());
    for user in users {
        names.push(match room.get_member_no_sync(user).await {
            // The display name, or the localpart if there's none.
            Ok(Some(member)) => member.name().to_owned(),
            _ => fallback_name(user),
        });
    }
    names
}

impl Watching {
    /// Mark everything up to the latest message read, which also clears a
    /// "marked unread" flag. Does nothing if it's already read.
    pub async fn mark_read(&self) -> Result<(), String> {
        self.timeline
            .mark_as_read(ReceiptType::Read)
            .await
            .map(|_| ())
            .map_err(|e| format!("Couldn't mark the conversation read: {e}"))
    }

    /// An image message's thumbnail as a `data:` URL.
    pub async fn thumbnail(&self, event_id: &str) -> Result<String, String> {
        let event_id = EventId::parse(event_id).map_err(|e| format!("Bad event ID: {e}"))?;
        let item = self
            .timeline
            .item_by_event_id(&event_id)
            .await
            .ok_or_else(|| "That message isn't loaded.".to_owned())?;
        let Some(MessageType::Image(image)) = item.content().as_message().map(|m| m.msgtype())
        else {
            return Err("That message isn't an image.".to_owned());
        };
        let info = image.info.as_deref();
        let (source, format) = match info.and_then(|i| i.thumbnail_source.clone()) {
            Some(thumbnail) => (thumbnail, MediaFormat::File),
            None => match &image.source {
                // The server shrinks it.
                MediaSource::Plain(_) => (
                    image.source.clone(),
                    MediaFormat::Thumbnail(MediaThumbnailSettings::new(
                        THUMBNAIL_WIDTH.into(),
                        THUMBNAIL_HEIGHT.into(),
                    )),
                ),
                // The server can't see inside, so it's the whole image.
                MediaSource::Encrypted(_) => {
                    if info
                        .and_then(|i| i.size)
                        .is_some_and(|s| u64::from(s) > MAX_FULL_IMAGE)
                    {
                        return Err("The image is too big to preview.".to_owned());
                    }
                    (image.source.clone(), MediaFormat::File)
                }
            },
        };
        let bytes = self
            .timeline
            .room()
            .client()
            .media()
            .get_media_content(&MediaRequestParameters { source, format }, true)
            .await
            .map_err(|e| format!("Couldn't load the image: {e}"))?;
        Ok(media::data_url(&bytes))
    }

    /// The file name an attachment message suggests saving it as.
    pub async fn attachment_name(&self, event_id: &str) -> Result<String, String> {
        let (name, _) = self.attachment(event_id).await?;
        Ok(safe_file_name(&name))
    }

    /// Download an attachment message's file, decrypted if need be, and
    /// write it to `path`.
    pub async fn save_attachment(&self, event_id: &str, path: PathBuf) -> Result<(), String> {
        let (_, source) = self.attachment(event_id).await?;
        let bytes = self
            .timeline
            .room()
            .client()
            .media()
            .get_media_content(
                &MediaRequestParameters {
                    source,
                    format: MediaFormat::File,
                },
                true,
            )
            .await
            .map_err(|e| format!("Couldn't download the file: {e}"))?;
        spawn_blocking(move || std::fs::write(&path, bytes))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| format!("Couldn't save the file: {e}"))
    }

    /// An attachment message's file name and where its file is.
    async fn attachment(&self, event_id: &str) -> Result<(String, MediaSource), String> {
        let event_id = EventId::parse(event_id).map_err(|e| format!("Bad event ID: {e}"))?;
        let item = self
            .timeline
            .item_by_event_id(&event_id)
            .await
            .ok_or_else(|| "That message isn't loaded.".to_owned())?;
        let (filename, body, source) = match item.content().as_message().map(|m| m.msgtype()) {
            Some(MessageType::Image(c)) => (&c.filename, &c.body, &c.source),
            Some(MessageType::Video(c)) => (&c.filename, &c.body, &c.source),
            Some(MessageType::Audio(c)) => (&c.filename, &c.body, &c.source),
            Some(MessageType::File(c)) => (&c.filename, &c.body, &c.source),
            _ => return Err("That message has no file.".to_owned()),
        };
        Ok((
            filename.clone().unwrap_or_else(|| body.clone()),
            source.clone(),
        ))
    }

    /// Load a page of older history; it arrives as diffs. Returns whether
    /// the start of the room was reached.
    pub async fn load_older(&self) -> Result<bool, String> {
        self.timeline
            .paginate_backwards(PAGE)
            .await
            .map_err(|e| format!("Couldn't load older messages: {e}"))
    }

    /// Tell the room whether we're typing. The SDK skips repeats, and the
    /// notice runs out by itself if it isn't renewed.
    pub async fn set_typing(&self, typing: bool) -> Result<(), String> {
        self.timeline
            .room()
            .typing_notice(typing)
            .await
            .map_err(|e| format!("Couldn't send the typing notice: {e}"))
    }

    /// The biggest file the server takes, in bytes, if it says.
    pub async fn max_upload_size(&self) -> Option<u64> {
        self.timeline
            .room()
            .client()
            .load_or_fetch_max_upload_size()
            .await
            .ok()
            .map(u64::from)
    }

    /// Upload a file and send it, encrypted first if the room is. `progress`
    /// gets (bytes sent, total) as the upload goes. There's no local echo:
    /// the message shows up once the server has it.
    pub async fn send_file(
        &self,
        file: upload::Prepared,
        progress: impl Fn(usize, usize),
    ) -> Result<(), String> {
        let name = file.name.clone();
        let send = self.timeline.send_attachment(
            AttachmentSource::Data {
                bytes: file.bytes,
                filename: file.name,
            },
            file.mime,
            file.config,
        );
        let mut updates = send.subscribe_to_send_progress();
        let mut send = std::pin::pin!(send.into_future());
        loop {
            tokio::select! {
                result = &mut send => {
                    return result.map_err(|e| format!("Couldn't send {name}: {e}"));
                }
                Some(update) = updates.next() => progress(update.current, update.total),
            }
        }
    }

    /// Send a message: plain text, plus formatted text that's cleaned first.
    /// It shows up in the lines right away and is encrypted first if the
    /// room is.
    pub async fn send(&self, body: &str, html: Option<&str>) -> Result<(), String> {
        let content = match html.map(html::sanitize) {
            Some(html) if !html.trim().is_empty() => RoomMessageEventContent::text_html(body, html),
            _ => RoomMessageEventContent::text_plain(body),
        };
        self.timeline
            .send(content.into())
            .await
            .map(|_| ())
            .map_err(|e| format!("Couldn't send the message: {e}"))
    }
}

impl Drop for Watching {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// If `item` replies to a message that isn't loaded, ask the SDK to load
/// it, once. The item then updates through a diff.
fn load_replied_to(
    timeline: &Arc<Timeline>,
    item: &TimelineItem,
    asked: &mut HashSet<OwnedEventId>,
) {
    let Some(event) = item.as_event() else {
        return;
    };
    let Some(reply) = event.content().in_reply_to() else {
        return;
    };
    // The SDK looks the reply up by its own ID, so it must have reached the
    // server; our own reply is asked about again once it has.
    let Some(reply_id) = event.event_id() else {
        return;
    };
    if !matches!(reply.event, TimelineDetails::Unavailable) || !asked.insert(reply_id.to_owned()) {
        return;
    }
    let (timeline, reply_id) = (timeline.clone(), reply_id.to_owned());
    spawn(async move {
        if let Err(e) = timeline.fetch_details_for_event(&reply_id).await {
            eprintln!(
                "Couldn't load the message {} replies to ({}): {e}",
                reply_id, reply.event_id
            );
        }
    });
}

/// The items a change adds or replaces.
fn diff_items(diff: &VectorDiff<Arc<TimelineItem>>) -> Vec<&TimelineItem> {
    match diff {
        VectorDiff::Append { values } | VectorDiff::Reset { values } => {
            values.iter().map(|item| &**item).collect()
        }
        VectorDiff::PushFront { value }
        | VectorDiff::PushBack { value }
        | VectorDiff::Insert { value, .. }
        | VectorDiff::Set { value, .. } => vec![&**value],
        _ => Vec::new(),
    }
}

fn diff(diff: VectorDiff<Arc<TimelineItem>>) -> Diff {
    match diff {
        VectorDiff::Append { values } => Diff::Append {
            values: values.iter().map(|item| line(item)).collect(),
        },
        VectorDiff::Clear => Diff::Clear,
        VectorDiff::PushFront { value } => Diff::PushFront {
            value: line(&value),
        },
        VectorDiff::PushBack { value } => Diff::PushBack {
            value: line(&value),
        },
        VectorDiff::PopFront => Diff::PopFront,
        VectorDiff::PopBack => Diff::PopBack,
        VectorDiff::Insert { index, value } => Diff::Insert {
            index,
            value: line(&value),
        },
        VectorDiff::Set { index, value } => Diff::Set {
            index,
            value: line(&value),
        },
        VectorDiff::Remove { index } => Diff::Remove { index },
        VectorDiff::Truncate { length } => Diff::Truncate { length },
        VectorDiff::Reset { values } => Diff::Reset {
            values: values.iter().map(|item| line(item)).collect(),
        },
    }
}

fn line(item: &TimelineItem) -> Line {
    let id = item.unique_id().0.clone();
    let Some(event) = item.as_event() else {
        return match item.as_virtual() {
            Some(VirtualTimelineItem::TimelineStart) => Line::Start { id },
            _ => Line::Other { id },
        };
    };
    let name = sender_name(event.sender_profile(), event.sender());
    let own = event.is_own();
    let timestamp = event.timestamp().get().into();
    let msg_like = match event.content() {
        TimelineItemContent::MsgLike(msg_like) => msg_like,
        TimelineItemContent::MembershipChange(membership) => {
            return match membership.change().and_then(MemberChange::from_sdk) {
                Some(change) => Line::Membership {
                    id,
                    name: membership
                        .display_name()
                        .unwrap_or_else(|| fallback_name(membership.user_id())),
                    change,
                    timestamp,
                },
                None => Line::Other { id },
            };
        }
        TimelineItemContent::OtherState(state) => {
            return match state.content() {
                AnyOtherStateEventContentChange::RoomTopic(StateEventContentChange::Original {
                    content,
                    ..
                }) => Line::Topic {
                    id,
                    name,
                    topic: Some(content.topic.trim())
                        .filter(|t| !t.is_empty())
                        .map(ToOwned::to_owned),
                    timestamp,
                },
                _ => Line::Other { id },
            };
        }
        _ => return Line::Other { id },
    };
    let message = match &msg_like.kind {
        MsgLikeKind::Message(message) => message,
        MsgLikeKind::Redacted => {
            return Line::Deleted {
                id,
                name,
                own,
                timestamp,
            };
        }
        MsgLikeKind::UnableToDecrypt(encrypted) => {
            let cause = match encrypted {
                EncryptedMessage::MegolmV1AesSha2 { cause, .. } => *cause,
                _ => UtdCause::Unknown,
            };
            return Line::Undecryptable {
                id,
                event_id: event.event_id().map(ToString::to_string),
                name,
                own,
                reason: UndecryptableReason::from_sdk(cause),
                timestamp,
            };
        }
        _ => return Line::Other { id },
    };

    let media = |kind, filename: &Option<String>, body: &str| Media {
        kind,
        name: filename.clone().unwrap_or_else(|| body.to_owned()),
        size: None,
        width: None,
        height: None,
    };
    let (emote, formatted, media) = match message.msgtype() {
        MessageType::Text(text) => (false, &text.formatted, None),
        MessageType::Notice(notice) => (false, &notice.formatted, None),
        MessageType::Emote(emote) => (true, &emote.formatted, None),
        MessageType::Image(c) => {
            let info = c.info.as_deref();
            let media = Media {
                size: info.and_then(|i| i.size).map(u64::from),
                width: info.and_then(|i| i.width).map(u64::from),
                height: info.and_then(|i| i.height).map(u64::from),
                ..media(MediaKind::Image, &c.filename, &c.body)
            };
            (false, &c.formatted, Some(media))
        }
        MessageType::Video(c) => {
            let info = c.info.as_deref();
            let media = Media {
                size: info.and_then(|i| i.size).map(u64::from),
                width: info.and_then(|i| i.width).map(u64::from),
                height: info.and_then(|i| i.height).map(u64::from),
                ..media(MediaKind::Video, &c.filename, &c.body)
            };
            (false, &c.formatted, Some(media))
        }
        MessageType::Audio(c) => {
            let media = Media {
                size: c.info.as_deref().and_then(|i| i.size).map(u64::from),
                ..media(MediaKind::Audio, &c.filename, &c.body)
            };
            (false, &c.formatted, Some(media))
        }
        MessageType::File(c) => {
            let media = Media {
                size: c.info.as_deref().and_then(|i| i.size).map(u64::from),
                ..media(MediaKind::File, &c.filename, &c.body)
            };
            (false, &c.formatted, Some(media))
        }
        _ => return Line::Other { id },
    };
    let html = formatted
        .as_ref()
        .filter(|f| f.format == MessageFormat::Html)
        .map(|f| html::sanitize(&f.body))
        .filter(|html| !html.trim().is_empty());
    let reply = msg_like
        .in_reply_to
        .as_ref()
        .map(|reply| match &reply.event {
            TimelineDetails::Ready(replied) => Reply {
                name: Some(sender_name(&replied.sender_profile, &replied.sender)),
                preview: Some(preview(&replied.content)),
            },
            _ => Reply {
                name: None,
                preview: None,
            },
        });
    Line::Message {
        id,
        event_id: event.event_id().map(ToString::to_string),
        sender: event.sender().to_string(),
        name,
        own,
        body: message.body().to_owned(),
        html,
        emote,
        edited: message.is_edited(),
        media,
        reply,
        failed: matches!(
            event.send_state(),
            Some(EventSendState::SendingFailed { .. })
        ),
        timestamp,
    }
}

/// The display name, or the localpart if there's none.
fn sender_name(profile: &TimelineDetails<Profile>, sender: &UserId) -> String {
    match profile {
        TimelineDetails::Ready(profile) => profile.display_name.clone(),
        _ => None,
    }
    .unwrap_or_else(|| fallback_name(sender))
}

/// One short line standing for a replied-to message.
fn preview(content: &TimelineItemContent) -> String {
    let text = match content.as_msglike().map(|m| &m.kind) {
        Some(MsgLikeKind::Message(message)) => match message.msgtype() {
            MessageType::Image(_) => "an image".to_owned(),
            MessageType::Video(_) => "a video".to_owned(),
            MessageType::Audio(_) => "an audio file".to_owned(),
            MessageType::File(_) => "a file".to_owned(),
            _ => message.body().to_owned(),
        },
        Some(MsgLikeKind::Redacted) => "a deleted message".to_owned(),
        Some(MsgLikeKind::UnableToDecrypt(_)) => "an encrypted message".to_owned(),
        _ => "a message".to_owned(),
    };
    shorten(&text.replace('\n', " "), PREVIEW_LENGTH)
}

/// `text` cut to `max` characters, with "…" if it was longer.
fn shorten(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

/// The last part of a sender-chosen file name, so a name like `../x` can't
/// pick the folder it's saved in.
fn safe_file_name(name: &str) -> String {
    // Senders on other systems may use either separator.
    name.rsplit(['/', '\\'])
        .next()
        .map(str::trim)
        .filter(|n| !n.is_empty() && *n != "." && *n != "..")
        .unwrap_or("attachment")
        .to_owned()
}

/// `@bob:example.org` → `bob`, for senders without a display name.
fn fallback_name(user_id: &UserId) -> String {
    user_id.localpart().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undecryptable_reasons_follow_the_cause() {
        use UndecryptableReason as R;
        assert_eq!(R::from_sdk(UtdCause::Unknown), R::Waiting);
        assert_eq!(
            R::from_sdk(UtdCause::HistoricalMessageAndBackupIsDisabled),
            R::BeforeSignOn
        );
        assert_eq!(R::from_sdk(UtdCause::SentBeforeWeJoined), R::BeforeJoined);
        assert_eq!(R::from_sdk(UtdCause::WithheldBySender), R::Withheld);
        assert_eq!(R::from_sdk(UtdCause::UnknownDevice), R::Untrusted);
        assert_eq!(
            serde_json::to_value(Line::Undecryptable {
                id: "1".into(),
                event_id: Some("$e".into()),
                name: "Bob".into(),
                own: false,
                reason: R::BeforeSignOn,
                timestamp: 5,
            })
            .unwrap(),
            serde_json::json!({"type": "undecryptable", "id": "1", "eventId": "$e", "name": "Bob",
                "own": false, "reason": "before-sign-on", "timestamp": 5})
        );
    }

    #[test]
    fn only_some_membership_changes_get_a_line() {
        use MembershipChange as C;
        assert_eq!(
            MemberChange::from_sdk(C::Joined),
            Some(MemberChange::Joined)
        );
        assert_eq!(
            MemberChange::from_sdk(C::InvitationAccepted),
            Some(MemberChange::Joined)
        );
        assert_eq!(MemberChange::from_sdk(C::Left), Some(MemberChange::Left));
        assert_eq!(
            MemberChange::from_sdk(C::KickedAndBanned),
            Some(MemberChange::Banned)
        );
        assert_eq!(MemberChange::from_sdk(C::InvitationRejected), None);
        assert_eq!(MemberChange::from_sdk(C::Knocked), None);
        assert_eq!(
            serde_json::to_value(Line::Membership {
                id: "1".into(),
                name: "Bob".into(),
                change: MemberChange::Joined,
                timestamp: 5,
            })
            .unwrap(),
            serde_json::json!({"type": "membership", "id": "1", "name": "Bob", "change": "joined", "timestamp": 5})
        );
    }

    #[test]
    fn fallback_name_is_the_localpart() {
        let bob = UserId::parse("@bob:example.org").unwrap();
        assert_eq!(fallback_name(&bob), "bob");
    }

    #[test]
    fn lines_serialize_for_the_window() {
        let line = Line::Message {
            id: "1".into(),
            event_id: Some("$e".into()),
            sender: "@bob:localhost".into(),
            name: "Bob Brown".into(),
            own: false,
            body: "cat.png".into(),
            html: None,
            emote: false,
            edited: true,
            media: Some(Media {
                kind: MediaKind::Image,
                name: "cat.png".into(),
                size: Some(1234),
                width: Some(640),
                height: None,
            }),
            reply: Some(Reply {
                name: Some("Alice Anderson".into()),
                preview: Some("send a cat".into()),
            }),
            failed: false,
            timestamp: 5,
        };
        assert_eq!(
            serde_json::to_value(Diff::Insert {
                index: 2,
                value: line
            })
            .unwrap(),
            serde_json::json!({
                "op": "insert",
                "index": 2,
                "value": {
                    "type": "message", "id": "1", "eventId": "$e",
                    "sender": "@bob:localhost", "name": "Bob Brown", "own": false,
                    "body": "cat.png", "html": null, "emote": false, "edited": true,
                    "media": {
                        "kind": "image", "name": "cat.png", "size": 1234,
                        "width": 640, "height": null,
                    },
                    "reply": { "name": "Alice Anderson", "preview": "send a cat" },
                    "failed": false, "timestamp": 5,
                },
            })
        );
        assert_eq!(
            serde_json::to_value(Diff::PushFront {
                value: Line::Other { id: "2".into() }
            })
            .unwrap(),
            serde_json::json!({ "op": "pushFront", "value": { "type": "other", "id": "2" } })
        );
    }

    #[test]
    fn file_names_cannot_pick_the_folder() {
        assert_eq!(safe_file_name("notes.txt"), "notes.txt");
        assert_eq!(safe_file_name("../../.bashrc"), ".bashrc");
        assert_eq!(safe_file_name("/etc/passwd"), "passwd");
        assert_eq!(safe_file_name("C:\\Windows\\evil.dll"), "evil.dll");
        assert_eq!(safe_file_name(".."), "attachment");
        assert_eq!(safe_file_name("dir/"), "attachment");
        assert_eq!(safe_file_name(""), "attachment");
    }

    #[test]
    fn previews_are_shortened() {
        assert_eq!(shorten("short", 10), "short");
        assert_eq!(shorten("exactly10!", 10), "exactly10!");
        assert_eq!(shorten("a bit too long", 5), "a bit…");
        // Counts characters, not bytes.
        assert_eq!(shorten("héllo wörld", 5), "héllo…");
    }
}
