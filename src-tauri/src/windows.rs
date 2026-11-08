//! Opening the app's windows. Each is focused instead of duplicated if it's
//! already open.

use tauri::{
    AppHandle, Manager, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent, Wry,
    webview::NewWindowResponse,
};

use crate::{matrix::rooms::Kind, preferences, session, tray};

/// Added to each window's content size for what AIMtrix draws around it:
/// the AIM-style title bar (18 px) and the 3 px window frame on each side
/// (`TitleBar.svelte`, `.aim-frame`).
const FRAME_WIDTH: f64 = 6.0;
const FRAME_HEIGHT: f64 = 24.0;

/// A window without the platform's title bar and frame: AIMtrix draws
/// AIM-style ones on every platform (PLAN.md 8.4).
///
/// Windows only ever show AIMtrix's own pages: navigating anywhere else (a
/// link in a message, say) is refused, and so are new windows, so no web
/// page can end up with access to the app's commands.
fn aim_window(
    app: &AppHandle,
    label: impl Into<String>,
    url: WebviewUrl,
) -> WebviewWindowBuilder<'_, Wry, AppHandle> {
    WebviewWindowBuilder::new(app, label, url)
        .decorations(false)
        .on_navigation(is_app_page)
        .on_new_window(|_, _| NewWindowResponse::Deny)
}

/// Whether `url` is one of AIMtrix's own pages: `tauri://localhost` (Linux,
/// macOS), `http(s)://tauri.localhost` (Windows), or the dev server in a
/// development build.
fn is_app_page(url: &Url) -> bool {
    match (url.scheme(), url.host_str()) {
        ("tauri", Some("localhost")) => true,
        ("http" | "https", Some("tauri.localhost")) => true,
        ("http", Some("localhost")) => cfg!(debug_assertions) && url.port() == Some(1420),
        ("about", _) => url.as_str() == "about:blank",
        _ => false,
    }
}

/// The Buddy List is the `main` window. Shows it again if it was hidden in
/// the tray.
pub fn open_buddy_list(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.unminimize()?;
        return window.set_focus();
    }
    let window = aim_window(app, "main", WebviewUrl::App("/".into()))
        .title("Buddy List")
        .inner_size(260.0 + FRAME_WIDTH, 540.0 + FRAME_HEIGHT)
        .min_inner_size(200.0 + FRAME_WIDTH, 300.0 + FRAME_HEIGHT)
        .build()?;
    // With "close to tray", closing only hides it, and AIMtrix keeps running.
    let app = app.clone();
    let hide = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event
            && tray::shown(&app)
            && app.state::<preferences::Store>().get().close_to_tray
        {
            api.prevent_close();
            if let Err(e) = hide.hide() {
                eprintln!("Couldn't hide the Buddy List: {e}");
            }
        }
    });
    Ok(())
}

/// Really close the Buddy List (signing off), even with "close to tray".
pub fn close_buddy_list(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main")
        && let Err(e) = window.destroy()
    {
        eprintln!("Couldn't close the Buddy List: {e}");
    }
}

/// What Sign On explains when it opens.
pub enum SignOnNotice {
    None,
    /// The server ended the session (signed out from another client).
    SignedOut,
    /// The saved session couldn't be resumed, and why. It's kept: the cause
    /// may be temporary, like a locked keyring.
    ResumeFailed(String),
}

pub fn open_sign_on(app: &AppHandle, notice: SignOnNotice) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("signon") {
        return window.set_focus();
    }
    let url = match notice {
        SignOnNotice::None => "/signon".to_owned(),
        SignOnNotice::SignedOut => "/signon?signed-out".to_owned(),
        SignOnNotice::ResumeFailed(reason) => {
            format!("/signon?resume-failed={}", encode_query(&reason))
        }
    };
    let window = aim_window(app, "signon", WebviewUrl::App(url.into()))
        .title("Sign On")
        .inner_size(260.0 + FRAME_WIDTH, 350.0 + FRAME_HEIGHT)
        .resizable(false)
        .center()
        .build()?;
    // A browser sign-on still waiting when Sign On closes is abandoned, not
    // left to finish unseen.
    let app = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            app.state::<crate::commands::BrowserSignOn>().cancel();
        }
    });
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
    aim_window(app, RECOVERY_LABEL, WebviewUrl::App("/recovery".into()))
        .title("Restore Messages")
        .inner_size(340.0 + FRAME_WIDTH, 250.0 + FRAME_HEIGHT)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}

/// Label of the Preferences window.
pub const PREFERENCES_LABEL: &str = "preferences";
pub const ABOUT_LABEL: &str = "about";

/// About AIMtrix: version, license, third-party licenses.
pub fn open_about(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(ABOUT_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    aim_window(app, ABOUT_LABEL, WebviewUrl::App("/about".into()))
        .title("About AIMtrix")
        .inner_size(300.0 + FRAME_WIDTH, 270.0 + FRAME_HEIGHT)
        .resizable(false)
        .center()
        .build()?;
    Ok(())
}

/// Preferences: auto-reply, the tray and fonts.
pub fn open_preferences(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(PREFERENCES_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    aim_window(
        app,
        PREFERENCES_LABEL,
        WebviewUrl::App("/preferences".into()),
    )
    .title("Preferences")
    .inner_size(340.0 + FRAME_WIDTH, 390.0 + FRAME_HEIGHT)
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
    aim_window(app, ADD_BUDDY_LABEL, WebviewUrl::App("/add-buddy".into()))
        .title("Add Buddy")
        .inner_size(300.0 + FRAME_WIDTH, 170.0 + FRAME_HEIGHT)
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
    aim_window(app, AWAY_LABEL, WebviewUrl::App("/away".into()))
        .title("Away Message")
        .inner_size(320.0 + FRAME_WIDTH, 260.0 + FRAME_HEIGHT)
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
    aim_window(
        app,
        SETUP_RECOVERY_LABEL,
        WebviewUrl::App("/setup-recovery".into()),
    )
    .title("Set Up Recovery")
    .inner_size(360.0 + FRAME_WIDTH, 290.0 + FRAME_HEIGHT)
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
    let window = aim_window(app, VERIFY_LABEL, WebviewUrl::App("/verify".into()))
        .title("Verify This Computer")
        .inner_size(360.0 + FRAME_WIDTH, 280.0 + FRAME_HEIGHT)
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
    let window = aim_window(app, label, WebviewUrl::App(url.into()))
        .title(style.title(name))
        .inner_size(style.size.0 + FRAME_WIDTH, style.size.1 + FRAME_HEIGHT)
        .min_inner_size(
            style.min_size.0 + FRAME_WIDTH,
            style.min_size.1 + FRAME_HEIGHT,
        )
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
    fn windows_stay_on_app_pages() {
        let ok = |u: &str| is_app_page(&Url::parse(u).unwrap());
        assert!(ok("tauri://localhost/im?room=%21a%3Alocalhost"));
        assert!(ok("http://tauri.localhost/"));
        assert!(ok("https://tauri.localhost/chat"));
        assert!(ok("about:blank"));
        assert!(!ok("https://example.org/"));
        assert!(!ok("http://localhost:8008/"));
        assert!(!ok("file:///etc/passwd"));
        assert!(!ok("tauri://evil.example/"));
    }

    #[test]
    fn query_values_are_encoded() {
        assert_eq!(encode_query("!abc:localhost"), "%21abc%3Alocalhost");
        assert_eq!(encode_query("a b&c=d"), "a%20b%26c%3Dd");
    }
}
