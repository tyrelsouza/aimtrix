//! Opening the app's windows. Each is focused instead of duplicated if it's
//! already open.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::{matrix::rooms::Kind, session};

/// The Buddy List is the `main` window.
pub fn open_buddy_list(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("/".into()))
        .title("Buddy List")
        .inner_size(260.0, 540.0)
        .min_inner_size(200.0, 300.0)
        .build()?;
    Ok(())
}

/// `signed_out` makes Sign On explain that the server ended the session.
pub fn open_sign_on(app: &AppHandle, signed_out: bool) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("signon") {
        return window.set_focus();
    }
    let url = if signed_out {
        "/signon?signed-out"
    } else {
        "/signon"
    };
    WebviewWindowBuilder::new(app, "signon", WebviewUrl::App(url.into()))
        .title("Sign On")
        .inner_size(250.0, 300.0)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}

/// Label of the Restore Messages window.
pub const RECOVERY_LABEL: &str = "recovery";

/// Restore Messages: asks for the recovery key so this device can read old
/// encrypted messages.
pub fn open_recovery(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(RECOVERY_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, RECOVERY_LABEL, WebviewUrl::App("/recovery".into()))
        .title("Restore Messages")
        .inner_size(340.0, 250.0)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}

/// Label of the Preferences window.
pub const PREFERENCES_LABEL: &str = "preferences";

/// Preferences: auto-reply and fonts.
pub fn open_preferences(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(PREFERENCES_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(
        app,
        PREFERENCES_LABEL,
        WebviewUrl::App("/preferences".into()),
    )
    .title("Preferences")
    .inner_size(340.0, 330.0)
    .resizable(false)
    .center()
    .build()?;
    Ok(())
}

/// Label of the Add Buddy window.
pub const ADD_BUDDY_LABEL: &str = "add-buddy";

/// Add Buddy: start a DM by Matrix ID.
pub fn open_add_buddy(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(ADD_BUDDY_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, ADD_BUDDY_LABEL, WebviewUrl::App("/add-buddy".into()))
        .title("Add Buddy")
        .inner_size(300.0, 170.0)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}

/// Label of the Away Message window.
pub const AWAY_LABEL: &str = "away";

/// Away Message: set an away message and go away.
pub fn open_away(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(AWAY_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, AWAY_LABEL, WebviewUrl::App("/away".into()))
        .title("Away Message")
        .inner_size(320.0, 240.0)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}

/// Label of the Set Up Recovery window.
pub const SETUP_RECOVERY_LABEL: &str = "setup-recovery";

/// Set Up Recovery: makes a recovery key for an account that has none.
pub fn open_setup_recovery(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETUP_RECOVERY_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(
        app,
        SETUP_RECOVERY_LABEL,
        WebviewUrl::App("/setup-recovery".into()),
    )
    .title("Set Up Recovery")
    .inner_size(360.0, 290.0)
    .resizable(false)
    .center()
    .build()?;
    Ok(())
}

/// Label of the Verify window.
pub const VERIFY_LABEL: &str = "verify";

/// Verify This Computer: compares emoji with another of your sessions.
pub fn open_verify(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(VERIFY_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    let window = WebviewWindowBuilder::new(app, VERIFY_LABEL, WebviewUrl::App("/verify".into()))
        .title("Verify This Computer")
        .inner_size(360.0, 280.0)
        .resizable(false)
        .center()
        .build()?;
    let app = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            session::end_verification(&app);
        }
    });
    Ok(())
}

/// Open the window for `room_id`: an IM for a DM, a Chat Room for a group.
/// There's one window per room; opening it again focuses it.
pub fn open_room(app: &AppHandle, room_id: &str, name: &str, kind: Kind) -> tauri::Result<()> {
    let label = room_label(room_id);
    if let Some(window) = app.get_webview_window(&label) {
        // Bring it back if it was minimized, not just focus it.
        window.unminimize()?;
        return window.set_focus();
    }
    let style = RoomWindow::for_kind(kind);
    let url = format!("{}?room={}", style.route, encode_query(room_id));
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title(style.title(name))
        .inner_size(style.size.0, style.size.1)
        .min_inner_size(style.min_size.0, style.min_size.1)
        .build()?;
    let (app, room_id) = (app.clone(), room_id.to_owned());
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            session::unwatch_timeline(&app, &room_id);
        }
    });
    Ok(())
}

/// How a room's window looks, by the kind of room.
#[derive(Debug, PartialEq)]
struct RoomWindow {
    route: &'static str,
    /// Added to the room's name in the title bar.
    title_suffix: &'static str,
    size: (f64, f64),
    min_size: (f64, f64),
}

impl RoomWindow {
    fn for_kind(kind: Kind) -> Self {
        match kind {
            Kind::Dm => Self {
                route: "/im",
                title_suffix: "",
                size: (420.0, 360.0),
                min_size: (280.0, 220.0),
            },
            // Wider, to leave room for the member list.
            Kind::Group => Self {
                route: "/chat",
                title_suffix: " - Chat Room",
                size: (540.0, 400.0),
                min_size: (360.0, 240.0),
            },
        }
    }

    fn title(&self, name: &str) -> String {
        format!("{name}{}", self.title_suffix)
    }
}

/// Prefix of every room window's label.
pub const ROOM_LABEL_PREFIX: &str = "room-";

/// `room-<key>`, the label of a room's window.
fn room_label(room_id: &str) -> String {
    format!("{ROOM_LABEL_PREFIX}{}", room_key(room_id))
}

/// Stands in for the room ID in window labels and event names, which can't
/// have characters like `!`.
pub fn room_key(room_id: &str) -> String {
    format!("{:016x}", fnv1a(room_id.as_bytes()))
}

/// FNV-1a: stable across runs and Rust versions, unlike `DefaultHasher`.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &b| {
        (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// Percent-encode a query string value.
fn encode_query(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                char::from(b).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_labels_are_stable_and_valid() {
        let label = room_label("!abc:localhost");
        assert_eq!(label, room_label("!abc:localhost"));
        assert_ne!(label, room_label("!abd:localhost"));
        assert_eq!(label, format!("room-{}", room_key("!abc:localhost")));
        assert_eq!(room_key("!abc:localhost").len(), 16);
        // Tauri allows alphanumerics, `-`, `/`, `:` and `_` in labels.
        assert!(label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    }

    #[test]
    fn fnv1a_matches_reference_values() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn groups_open_chat_rooms_and_dms_open_ims() {
        let im = RoomWindow::for_kind(Kind::Dm);
        assert_eq!(im.route, "/im");
        assert_eq!(im.title("Bob Brown"), "Bob Brown");
        let chat = RoomWindow::for_kind(Kind::Group);
        assert_eq!(chat.route, "/chat");
        assert_eq!(chat.title("Test Chat"), "Test Chat - Chat Room");
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode_query("!abc:localhost"), "%21abc%3Alocalhost");
        assert_eq!(encode_query("a b&c=d"), "a%20b%26c%3Dd");
    }
}
