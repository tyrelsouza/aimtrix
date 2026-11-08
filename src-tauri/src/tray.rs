//! The tray icon on Windows and Linux (PLAN.md 8.6): your status at a
//! glance, a menu (Open Buddy List, Away…/I'm Back, Sign Off, Quit), and
//! with the "close to tray" preference, somewhere for AIMtrix to keep
//! running when the Buddy List is closed. macOS has the Dock instead.
//!
//! Linux trays need AppIndicator, which not every desktop has (stock GNOME
//! needs an extension, i3 a bar with a tray). Without its library there's
//! no tray, and closing the Buddy List quits as before.

use std::sync::Mutex;

use tauri::{
    AppHandle, Manager,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

use crate::{session, windows};

const ID: &str = "aimtrix";

/// What the tray shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    /// The signed-on user ID, if signed on.
    pub user: Option<String>,
    /// Whether we're away; `None` on servers without presence.
    pub away: Option<bool>,
    /// Conversations with unread messages.
    pub unread: usize,
}

/// The tray's state, kept so it's only redrawn when something changes.
#[derive(Default)]
pub struct Tray(Mutex<State>);

/// Whether this system can show a tray icon.
pub fn available() -> bool {
    #[cfg(target_os = "linux")]
    {
        // The tray library panics if neither is there; check first.
        [
            "libayatana-appindicator3.so.1",
            "libappindicator3.so.1",
            "libayatana-appindicator3.so",
            "libappindicator3.so",
        ]
        .iter()
        // SAFETY: loading a shared library runs its initializers; these are
        // the system's AppIndicator libraries, which the tray loads anyway.
        .any(|name| unsafe { libloading::Library::new(name) }.is_ok())
    }
    #[cfg(target_os = "windows")]
    {
        true
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        false
    }
}

/// Whether the tray icon is there (so closing the Buddy List can hide it).
pub fn shown(app: &AppHandle) -> bool {
    app.tray_by_id(ID).is_some()
}

/// Add the tray icon, if this system can show one.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    if !available() {
        return Ok(());
    }
    let tray_state = app.state::<Tray>();
    let state = tray_state.0.lock().unwrap().clone();
    TrayIconBuilder::with_id(ID)
        .icon(icon(&state))
        .tooltip(tooltip(&state))
        .menu(&menu(app, &state)?)
        // Left click opens the Buddy List (Windows; Linux trays only have
        // the menu), right click the menu.
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                open(tray.app_handle().clone());
            }
        })
        .build(app)?;
    Ok(())
}

/// Change what the tray shows, redrawing it if anything changed.
pub fn update(app: &AppHandle, change: impl FnOnce(&mut State)) {
    let tray_state = app.state::<Tray>();
    {
        let mut current = tray_state.0.lock().unwrap();
        let before = current.clone();
        change(&mut current);
        if *current == before {
            return;
        }
    }
    // Drawn from the state as it is now, not as this change left it: when
    // two updates race, whichever draws last still draws the newest.
    let state = tray_state.0.lock().unwrap().clone();
    #[cfg(target_os = "macos")]
    dock_badge(app, &state);
    let Some(tray) = app.tray_by_id(ID) else {
        return;
    };
    let redrawn = tray
        .set_icon(Some(icon(&state)))
        .and_then(|()| tray.set_tooltip(Some(tooltip(&state))))
        .and_then(|()| tray.set_menu(Some(menu(app, &state)?)));
    if let Err(e) = redrawn {
        eprintln!("Couldn't update the tray icon: {e}");
    }
}

/// On macOS, unread conversations show as a count on the Dock icon.
#[cfg(target_os = "macos")]
fn dock_badge(app: &AppHandle, state: &State) {
    if let Some(window) = app.get_webview_window("main") {
        let count = (state.unread > 0).then_some(state.unread as i64);
        if let Err(e) = window.set_badge_count(count) {
            eprintln!("Couldn't badge the Dock icon: {e}");
        }
    }
}

fn menu(app: &AppHandle, state: &State) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    if state.user.is_some() {
        menu.append(&MenuItem::with_id(
            app,
            "open",
            "Open Buddy List",
            true,
            None::<&str>,
        )?)?;
        let item = match state.away {
            Some(true) => MenuItem::with_id(app, "back", "I'm Back", true, None::<&str>)?,
            Some(false) => MenuItem::with_id(app, "away", "Away…", true, None::<&str>)?,
            // No presence on this server: nothing to be away with.
            None => MenuItem::with_id(app, "away", "Away…", false, None::<&str>)?,
        };
        menu.append(&item)?;
        menu.append(&MenuItem::with_id(
            app,
            "signoff",
            "Sign Off",
            true,
            None::<&str>,
        )?)?;
    } else {
        menu.append(&MenuItem::with_id(
            app,
            "open",
            "Sign On…",
            true,
            None::<&str>,
        )?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit AIMtrix",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

/// Runs on the main thread, which mustn't create windows itself (that can
/// deadlock on Windows), so window work is spawned.
fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => open(app.clone()),
        "away" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = windows::open_away(&app) {
                    eprintln!("Couldn't open Away Message: {e}");
                }
            });
        }
        "back" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = session::set_away(&app, None).await {
                    eprintln!("Couldn't come back: {e}");
                }
            });
        }
        "signoff" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { session::sign_off(&app).await });
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Open the Buddy List when signed on, Sign On otherwise (the tray, and
/// the Dock on macOS). Called on the main thread, so the window is opened
/// from a task: creating one on the main thread can deadlock on Windows.
pub fn open(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let signed_on = app.state::<Tray>().0.lock().unwrap().user.is_some();
        let opened = if signed_on {
            windows::open_buddy_list(&app)
        } else {
            windows::open_sign_on(&app, windows::SignOnNotice::None)
        };
        if let Err(e) = opened {
            eprintln!("Couldn't open a window: {e}");
        }
    });
}

fn tooltip(state: &State) -> String {
    let status = match (&state.user, state.away) {
        (None, _) => return "AIMtrix: signed off".to_owned(),
        (Some(_), Some(true)) => "away",
        (Some(_), _) => "online",
    };
    let user = state.user.as_deref().unwrap_or_default();
    match state.unread {
        0 => format!("AIMtrix: {user}, {status}"),
        1 => format!("AIMtrix: {user}, {status}\n1 unread conversation"),
        n => format!("AIMtrix: {user}, {status}\n{n} unread conversations"),
    }
}

/// The app icon, marked up for `state`.
fn icon(state: &State) -> Image<'static> {
    const PNG: &[u8] = include_bytes!("../icons/64x64.png");
    let base = image::load_from_memory_with_format(PNG, image::ImageFormat::Png)
        .expect("the bundled icon is a PNG")
        .to_rgba8();
    let (width, height) = base.dimensions();
    let mut rgba = base.into_raw();
    mark(&mut rgba, width, state);
    Image::new_owned(rgba, width, height)
}

/// Signed off: grey and faded. Away: a slate dot at the bottom right.
/// Unread messages: a red dot at the top right.
fn mark(rgba: &mut [u8], size: u32, state: &State) {
    if state.user.is_none() {
        for pixel in rgba.as_chunks_mut::<4>().0 {
            let grey =
                (u32::from(pixel[0]) * 3 + u32::from(pixel[1]) * 6 + u32::from(pixel[2])) / 10;
            pixel[..3].fill(grey as u8);
            pixel[3] = (u32::from(pixel[3]) * 3 / 5) as u8;
        }
        return;
    }
    let radius = size as f32 * 0.2;
    let edge = radius + size as f32 / 32.0 + 0.5;
    if state.away == Some(true) {
        dot(
            rgba,
            size,
            (size as f32 - edge, size as f32 - edge),
            radius,
            [0x4a, 0x5a, 0x6a],
        );
    }
    if state.unread > 0 {
        dot(
            rgba,
            size,
            (size as f32 - edge, edge),
            radius,
            [0xd0, 0x02, 0x1b],
        );
    }
}

/// A filled circle of `color` with a white ring, centred on `center`.
fn dot(rgba: &mut [u8], size: u32, center: (f32, f32), radius: f32, color: [u8; 3]) {
    let ring = radius + size as f32 / 32.0;
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - center.0, y as f32 + 0.5 - center.1);
            let distance = (dx * dx + dy * dy).sqrt();
            let rgb = if distance <= radius {
                color
            } else if distance <= ring {
                [0xff; 3]
            } else {
                continue;
            };
            let at = ((y * size + x) * 4) as usize;
            rgba[at..at + 3].copy_from_slice(&rgb);
            rgba[at + 3] = 0xff;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &Image<'_>, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width() + x) * 4) as usize;
        image.rgba()[at..at + 4].try_into().unwrap()
    }

    #[test]
    fn the_icon_shows_status_and_unread_messages() {
        let online = State {
            user: Some("@alice:localhost".into()),
            away: Some(false),
            unread: 0,
        };
        let plain = icon(&online);
        let unread = icon(&State {
            unread: 2,
            ..online.clone()
        });
        let away = icon(&State {
            away: Some(true),
            ..online.clone()
        });
        let (top_right, bottom_right) = ((52, 11), (52, 52));

        assert_eq!(
            pixel(&unread, top_right.0, top_right.1),
            [0xd0, 0x02, 0x1b, 0xff]
        );
        assert_ne!(
            pixel(&plain, top_right.0, top_right.1),
            [0xd0, 0x02, 0x1b, 0xff]
        );
        assert_eq!(
            pixel(&away, bottom_right.0, bottom_right.1),
            [0x4a, 0x5a, 0x6a, 0xff]
        );
        assert_eq!(
            pixel(&unread, bottom_right.0, bottom_right.1),
            pixel(&plain, 52, 52)
        );

        // Signed off: every pixel grey.
        let off = icon(&State::default());
        assert!(
            off.rgba()
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| p[0] == p[1] && p[1] == p[2])
        );
    }

    #[test]
    fn the_tooltip_says_who_and_how() {
        let state = State {
            user: Some("@alice:localhost".into()),
            away: Some(true),
            unread: 3,
        };
        assert_eq!(
            tooltip(&state),
            "AIMtrix: @alice:localhost, away\n3 unread conversations"
        );
        assert_eq!(
            tooltip(&State {
                away: None,
                unread: 1,
                ..state.clone()
            }),
            "AIMtrix: @alice:localhost, online\n1 unread conversation"
        );
        assert_eq!(tooltip(&State::default()), "AIMtrix: signed off");
    }
}
