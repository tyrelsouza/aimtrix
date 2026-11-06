//! A room's conversation, as lines for an IM window.
//!
//! [`watch`] follows a room's [`Timeline`] and turns each change into a
//! [`Diff`] of [`Line`]s. Every timeline item maps to exactly one line, so
//! the window can apply the diffs by index. [`Watching::send`] sends
//! through the same timeline, so a sent message shows up at once.

use eyeball_im::VectorDiff;
use futures_util::StreamExt;
use matrix_sdk::{
    Client, RoomState,
    ruma::{
        RoomId, UserId,
        events::room::message::{MessageType, RoomMessageEventContent},
    },
};
use matrix_sdk_ui::timeline::{EventSendState, RoomExt, Timeline, TimelineDetails, TimelineItem};
use serde::Serialize;
use std::sync::Arc;
use tauri::async_runtime::{JoinHandle, spawn};

/// History to load when the window opens.
const FIRST_PAGE: u16 = 50;

/// One line of the conversation.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Line {
    /// "Name: text".
    Message {
        id: String,
        sender: String,
        name: String,
        /// Sent by us.
        own: bool,
        body: String,
        /// `/me`: shown as "* Name text".
        emote: bool,
        /// Ours, and the server didn't take it.
        failed: bool,
        /// Milliseconds since the Unix epoch.
        timestamp: u64,
    },
    /// Not shown yet: other message types, state events, date dividers…
    Other { id: String },
}

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

    let task = spawn({
        let timeline = timeline.clone();
        async move {
            // Both send their changes through `diffs`, so run them alongside.
            // Sync loads members lazily, so without fetching them senders can
            // lack display names.
            let setup = async {
                let (history, ()) = tokio::join!(
                    timeline.paginate_backwards(FIRST_PAGE),
                    timeline.fetch_members()
                );
                if let Err(e) = history {
                    eprintln!(
                        "Couldn't load history for {}: {e}",
                        timeline.room().room_id()
                    );
                }
            };
            let mut setup = std::pin::pin!(setup);
            let mut setting_up = true;
            loop {
                tokio::select! {
                    () = &mut setup, if setting_up => setting_up = false,
                    batch = diffs.next() => match batch {
                        Some(batch) => batch.into_iter().map(diff).for_each(&on_diff),
                        None => break,
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

impl Watching {
    /// Send a plain-text message. It shows up in the lines right away and is
    /// encrypted first if the room is.
    pub async fn send(&self, body: &str) -> Result<(), String> {
        self.timeline
            .send(RoomMessageEventContent::text_plain(body).into())
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
        return Line::Other { id };
    };
    let Some(message) = event.content().as_message() else {
        return Line::Other { id };
    };
    let emote = match message.msgtype() {
        MessageType::Text(_) | MessageType::Notice(_) => false,
        MessageType::Emote(_) => true,
        _ => return Line::Other { id },
    };
    let name = match event.sender_profile() {
        TimelineDetails::Ready(profile) => profile.display_name.clone(),
        _ => None,
    }
    .unwrap_or_else(|| fallback_name(event.sender()));
    Line::Message {
        id,
        sender: event.sender().to_string(),
        name,
        own: event.is_own(),
        body: message.body().to_owned(),
        emote,
        failed: matches!(
            event.send_state(),
            Some(EventSendState::SendingFailed { .. })
        ),
        timestamp: event.timestamp().get().into(),
    }
}

/// `@bob:example.org` → `bob`, for senders without a display name.
fn fallback_name(user_id: &UserId) -> String {
    user_id.localpart().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_name_is_the_localpart() {
        let bob = UserId::parse("@bob:example.org").unwrap();
        assert_eq!(fallback_name(&bob), "bob");
    }

    #[test]
    fn lines_serialize_for_the_window() {
        let line = Line::Message {
            id: "1".into(),
            sender: "@bob:localhost".into(),
            name: "Bob Brown".into(),
            own: false,
            body: "hi".into(),
            emote: false,
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
                    "type": "message", "id": "1", "sender": "@bob:localhost",
                    "name": "Bob Brown", "own": false, "body": "hi",
                    "emote": false, "failed": false, "timestamp": 5,
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
}
