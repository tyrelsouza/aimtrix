//! Opening the app's windows. Each is focused instead of duplicated if it's
//! already open.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::session;

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

/// Open an IM window for `room_id`, titled `name`. There's one window per
/// room; opening it again focuses it.
pub fn open_room(app: &AppHandle, room_id: &str, name: &str) -> tauri::Result<()> {
    let label = room_label(room_id);
    if let Some(window) = app.get_webview_window(&label) {
        // Bring it back if it was minimized, not just focus it.
        window.unminimize()?;
        return window.set_focus();
    }
    let url = format!("/im?room={}", encode_query(room_id));
    let window = WebviewWindowBuilder::new(app, label, WebviewUrl::App(url.into()))
        .title(name)
        .inner_size(420.0, 360.0)
        .min_inner_size(280.0, 220.0)
        .build()?;
    let (app, room_id) = (app.clone(), room_id.to_owned());
    window.on_window_event(move |event| {
        if let WindowEvent::Destroyed = event {
            session::unwatch_timeline(&app, &room_id);
        }
    });
    Ok(())
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
    fn query_values_are_encoded() {
        assert_eq!(encode_query("!abc:localhost"), "%21abc%3Alocalhost");
        assert_eq!(encode_query("a b&c=d"), "a%20b%26c%3Dd");
    }
}
