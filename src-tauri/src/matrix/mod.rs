//! The single Matrix session owned by the Rust core: sign on, resume, sign off.

mod html;
mod keyring;
pub mod media;
pub mod rooms;
pub mod sync;
pub mod timeline;

use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
};

use matrix_sdk::{Client, ruma::api::error::ErrorKind};
use rand::distr::{Alphanumeric, SampleString};
use tauri::async_runtime::{Mutex, spawn_blocking};

use keyring::SavedSession;

/// App-wide session state, shared by every window.
#[derive(Default)]
pub struct Session {
    /// The signed-on client and its sync. `None` until sign-on.
    pub signed: Mutex<Option<Signed>>,
    /// The latest connection status, for windows that open after it changed.
    pub status: std::sync::Mutex<sync::Status>,
    /// The latest Buddy List, for windows that open after it changed.
    pub buddies: std::sync::Mutex<Vec<rooms::Buddy>>,
    /// The conversation each open room window follows, by room ID.
    pub timelines: std::sync::Mutex<HashMap<String, std::sync::Arc<timeline::Watching>>>,
}

pub struct Signed {
    pub client: Client,
    pub sync: sync::Syncing,
    pub buddy_list: tauri::async_runtime::JoinHandle<()>,
}

fn store_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("matrix-store")
}

/// Sign on with a password, and save the session so the next launch can resume.
pub async fn sign_on(
    data_dir: &Path,
    homeserver: &str,
    username: &str,
    password: &str,
) -> Result<Client, String> {
    // A store belongs to one device, so every sign-on starts a fresh one.
    let store = store_dir(data_dir);
    remove_store(&store)?;
    let passphrase = Alphanumeric.sample_string(&mut rand::rng(), 32);

    let client = login(&store, &passphrase, homeserver, username, password).await?;
    let saved = SavedSession {
        homeserver: client.homeserver().to_string(),
        store_passphrase: passphrase,
        session: client
            .matrix_auth()
            .session()
            .expect("logged in client has a session"),
    };
    spawn_blocking(move || keyring::save(&saved))
        .await
        .map_err(|e| e.to_string())??;
    Ok(client)
}

/// Resume the saved session, if there is one. Works offline: the token is
/// only checked once syncing starts.
pub async fn resume(data_dir: &Path) -> Result<Option<Client>, String> {
    let Some(saved) = spawn_blocking(keyring::load)
        .await
        .map_err(|e| e.to_string())??
    else {
        return Ok(None);
    };
    restore(&store_dir(data_dir), &saved).await.map(Some)
}

/// Sign off: delete this device on the server, then forget it locally. Local
/// cleanup happens even if the server can't be reached.
pub async fn sign_off(data_dir: &Path, client: Client) -> Result<(), String> {
    if let Err(e) = client.logout().await {
        eprintln!("Server-side logout failed, forgetting the session anyway: {e}");
    }
    drop(client);
    forget(data_dir).await
}

/// Forget the saved session and its store without contacting the server.
pub async fn forget(data_dir: &Path) -> Result<(), String> {
    spawn_blocking(keyring::forget)
        .await
        .map_err(|e| e.to_string())??;
    remove_store(&store_dir(data_dir))
}

/// Log in with a password into a new store at `store`.
///
/// `homeserver` is a server name (discovered via `.well-known`) or a full URL.
async fn login(
    store: &Path,
    passphrase: &str,
    homeserver: &str,
    username: &str,
    password: &str,
) -> Result<Client, String> {
    let client = Client::builder()
        .server_name_or_homeserver_url(homeserver)
        .sqlite_store(store, Some(passphrase))
        .build()
        .await
        .map_err(|e| format!("Couldn't reach homeserver: {e}"))?;

    client
        .matrix_auth()
        .login_username(username, password)
        .initial_device_display_name("AIMtrix")
        .await
        .map_err(|e| match e.client_api_error_kind() {
            Some(ErrorKind::Forbidden) => "Incorrect screen name or password.".to_owned(),
            Some(ErrorKind::UserDeactivated) => "This account has been deactivated.".to_owned(),
            _ => format!("Sign on failed: {e}"),
        })?;

    Ok(client)
}

/// Reopen the store at `store` and restore the saved session into it.
async fn restore(store: &Path, saved: &SavedSession) -> Result<Client, String> {
    let client = Client::builder()
        .homeserver_url(&saved.homeserver)
        .sqlite_store(store, Some(&saved.store_passphrase))
        .build()
        .await
        .map_err(|e| format!("Couldn't open the session store: {e}"))?;
    client
        .restore_session(saved.session.clone())
        .await
        .map_err(|e| format!("Couldn't restore the session: {e}"))?;
    Ok(client)
}

fn remove_store(store: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(store) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Couldn't clear the old session store: {e}")),
    }
}

/// Tests marked `ignore` need the dev homeserver: `docker compose -f
/// dev/docker-compose.yml up -d`, `python3 dev/seed.py`, then
/// `cargo test -- --include-ignored`. None of them touch the OS keyring.
#[cfg(test)]
mod tests {
    use super::*;

    const HS: &str = "http://localhost:8008";
    const PASS: &str = "store-passphrase";

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn login_returns_user_id() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        assert_eq!(client.user_id().unwrap(), "@alice:localhost");
        // Don't leave a device behind on the test account.
        client.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn login_rejects_bad_password() {
        let dir = tempfile::tempdir().unwrap();
        let err = login(dir.path(), PASS, HS, "alice", "wrong")
            .await
            .unwrap_err();
        assert_eq!(err, "Incorrect screen name or password.");
    }

    #[tokio::test]
    async fn login_rejects_unreachable_homeserver() {
        let dir = tempfile::tempdir().unwrap();
        let err = login(dir.path(), PASS, "http://127.0.0.1:9", "alice", "x")
            .await
            .unwrap_err();
        assert!(err.starts_with("Couldn't reach homeserver"), "{err}");
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn restore_resumes_session_from_store() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "bob", "aimtrix-test")
            .await
            .unwrap();
        let mut saved = SavedSession {
            homeserver: client.homeserver().to_string(),
            store_passphrase: PASS.to_owned(),
            session: client.matrix_auth().session().unwrap(),
        };
        drop(client);

        saved.store_passphrase = "wrong".to_owned();
        assert!(
            restore(dir.path(), &saved).await.is_err(),
            "wrong passphrase must not open the store"
        );

        saved.store_passphrase = PASS.to_owned();
        let client = restore(dir.path(), &saved).await.unwrap();
        // A real server round trip proves the restored token works.
        assert_eq!(client.whoami().await.unwrap().user_id, "@bob:localhost");
        client.logout().await.unwrap();
    }

    /// Wait up to 20 s for `want` to arrive on `rx`.
    async fn wait_for(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<sync::Status>,
        want: sync::Status,
    ) {
        let found = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            while let Some(status) = rx.recv().await {
                if status == want {
                    return;
                }
            }
            panic!("status stream ended before {want:?}");
        });
        found
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {want:?}"));
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn sync_goes_online() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "carol", "aimtrix-test")
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let syncing = sync::start(&client, move |s| tx.send(s).unwrap())
            .await
            .unwrap();

        wait_for(&mut rx, sync::Status::Connecting).await;
        wait_for(&mut rx, sync::Status::Online).await;

        syncing.stop().await;
        client.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn sync_reports_revoked_token_as_signed_out() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "carol", "aimtrix-test")
            .await
            .unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let syncing = sync::start(&client, move |s| tx.send(s).unwrap())
            .await
            .unwrap();
        wait_for(&mut rx, sync::Status::Online).await;

        // Revoke the token from "another client" sharing the same session.
        let other_dir = tempfile::tempdir().unwrap();
        let saved = SavedSession {
            homeserver: client.homeserver().to_string(),
            store_passphrase: PASS.to_owned(),
            session: client.matrix_auth().session().unwrap(),
        };
        restore(other_dir.path(), &saved)
            .await
            .unwrap()
            .logout()
            .await
            .unwrap();

        wait_for(&mut rx, sync::Status::SignedOut).await;
        syncing.stop().await;
    }

    #[test]
    fn remove_store_ignores_missing_dir() {
        let dir = tempfile::tempdir().unwrap();
        remove_store(&dir.path().join("nope")).unwrap();
        remove_store(dir.path()).unwrap();
        assert!(!dir.path().exists());
    }

    /// Wait up to 20 s for a Buddy List on `rx` that passes `want`.
    async fn wait_for_buddies(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<Vec<rooms::Buddy>>,
        what: &str,
        want: impl Fn(&[rooms::Buddy]) -> bool,
    ) {
        let found = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            let mut last = Vec::new();
            while let Some(buddies) = rx.recv().await {
                if want(&buddies) {
                    return;
                }
                last = buddies;
            }
            panic!("Buddy List stream ended before {what}; last: {last:#?}");
        });
        found
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }

    fn has(buddies: &[rooms::Buddy], name: &str, section: rooms::Section) -> bool {
        buddies
            .iter()
            .any(|b| b.name == name && b.section == section)
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn buddy_list_shows_seeded_rooms_and_updates_live() {
        use matrix_sdk::ruma::{api::client::room::create_room::v3::Request, user_id};
        use rooms::Section;

        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = rooms::watch(syncing.room_list_service(), move |b| {
            let _ = tx.send(b);
        });

        wait_for_buddies(&mut rx, "the seeded rooms", |b| {
            has(b, "Bob Brown", Section::Buddies)
                && has(b, "Carol Chen", Section::Buddies)
                && has(b, "Test Chat", Section::Favorites)
                && has(b, "Secret Chat", Section::Chats)
                && has(b, "Invite Test", Section::Invitations)
        })
        .await;

        // Bob invites Alice to a new room; it must show up without restarting.
        let bob_dir = tempfile::tempdir().unwrap();
        let bob = login(bob_dir.path(), PASS, HS, "bob", "aimtrix-test")
            .await
            .unwrap();
        let mut request = Request::new();
        request.name = Some("Live Update Test".to_owned());
        request.invite = vec![user_id!("@alice:localhost").to_owned()];
        let room = bob.create_room(request).await.unwrap();

        wait_for_buddies(&mut rx, "the new invite", |b| {
            has(b, "Live Update Test", Section::Invitations)
        })
        .await;

        // Clean up: Alice declines, Bob leaves, both sign off.
        let invite = alice.get_room(room.room_id()).unwrap();
        invite.leave().await.unwrap();
        wait_for_buddies(&mut rx, "the declined invite to go", |b| {
            !b.iter().any(|b| b.name == "Live Update Test")
        })
        .await;
        room.leave().await.unwrap();
        bob.logout().await.unwrap();

        watcher.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }

    fn find<'a>(buddies: &'a [rooms::Buddy], name: &str) -> Option<(usize, &'a rooms::Buddy)> {
        buddies.iter().enumerate().find(|(_, b)| b.name == name)
    }

    /// `name` is unread and listed above `other`.
    fn unread_above(buddies: &[rooms::Buddy], name: &str, other: &str) -> bool {
        match (find(buddies, name), find(buddies, other)) {
            (Some((i, b)), Some((j, _))) => b.unread && i < j,
            _ => false,
        }
    }

    /// Sign `user` on, catch up once, and send `body` to `room_id`.
    async fn send_as(user: &str, room_id: &str, body: &str) {
        use matrix_sdk::{
            config::SyncSettings,
            ruma::{RoomId, events::room::message::RoomMessageEventContent},
        };
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, user, "aimtrix-test")
            .await
            .unwrap();
        client.sync_once(SyncSettings::default()).await.unwrap();
        let room = client
            .get_room(&RoomId::parse(room_id).unwrap())
            .unwrap_or_else(|| panic!("{user} isn't in {room_id}"));
        room.send(RoomMessageEventContent::text_plain(body))
            .await
            .unwrap();
        client.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn new_message_bolds_buddy_and_moves_it_up() {
        use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;

        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = rooms::watch(syncing.room_list_service(), move |b| {
            let _ = tx.send(b);
        });

        let room_ids = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        wait_for_buddies(&mut rx, "both DMs", {
            let room_ids = room_ids.clone();
            move |b| {
                let ids: Vec<_> = ["Bob Brown", "Carol Chen"]
                    .iter()
                    .filter_map(|name| find(b, name).map(|(_, b)| b.room_id.clone()))
                    .collect();
                let found = ids.len() == 2;
                *room_ids.lock().unwrap() = ids;
                found
            }
        })
        .await;
        let [bob_dm, carol_dm] = room_ids.lock().unwrap().clone().try_into().unwrap();

        // Alice talks in both, which marks everything before it read.
        for id in [&bob_dm, &carol_dm] {
            let room = alice
                .get_room(&matrix_sdk::ruma::RoomId::parse(id).unwrap())
                .unwrap();
            room.send(RoomMessageEventContent::text_plain("unread test: hi"))
                .await
                .unwrap();
        }
        wait_for_buddies(&mut rx, "both DMs read", |b| {
            ["Bob Brown", "Carol Chen"]
                .iter()
                .all(|name| find(b, name).is_some_and(|(_, b)| !b.unread))
        })
        .await;

        send_as("bob", &bob_dm, "unread test: from bob").await;
        wait_for_buddies(&mut rx, "Bob unread and on top", |b| {
            unread_above(b, "Bob Brown", "Carol Chen")
        })
        .await;

        // Carol's DM is encrypted, so this also covers counting decrypted messages.
        send_as("carol", &carol_dm, "unread test: from carol").await;
        wait_for_buddies(&mut rx, "Carol unread and on top", |b| {
            unread_above(b, "Carol Chen", "Bob Brown")
        })
        .await;

        watcher.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn buddy_icons_load_for_rooms_with_avatars() {
        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = rooms::watch(syncing.room_list_service(), move |b| {
            let _ = tx.send(b);
        });

        let icon = |b: &[rooms::Buddy], name: &str| find(b, name).map(|(_, b)| b.icon.clone());
        // Seeded avatars are PNGs: Bob's profile picture shows in his DM, and
        // Test Chat has a room avatar. Carol and Secret Chat have none.
        wait_for_buddies(&mut rx, "icons for Bob and Test Chat", |b| {
            let png = |name| {
                icon(b, name)
                    .flatten()
                    .is_some_and(|i| i.starts_with("data:image/png;base64,"))
            };
            png("Bob Brown")
                && png("Test Chat")
                && icon(b, "Carol Chen") == Some(None)
                && icon(b, "Secret Chat") == Some(None)
        })
        .await;

        watcher.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }

    fn section_of(buddies: &[rooms::Buddy], room_id: &str) -> Option<rooms::Section> {
        buddies
            .iter()
            .find(|b| b.room_id == room_id)
            .map(|b| b.section)
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn accepting_invites_moves_them_to_the_right_group() {
        use matrix_sdk::ruma::{
            api::client::room::create_room::v3::{Request, RoomPreset},
            user_id,
        };
        use rooms::Section;

        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = rooms::watch(syncing.room_list_service(), move |b| {
            let _ = tx.send(b);
        });

        let bob_dir = tempfile::tempdir().unwrap();
        let bob = login(bob_dir.path(), PASS, HS, "bob", "aimtrix-test")
            .await
            .unwrap();
        let invite = |name: Option<&str>, direct: bool, carol_too: bool| {
            let mut request = Request::new();
            request.name = name.map(ToOwned::to_owned);
            request.is_direct = direct;
            request.preset = Some(RoomPreset::TrustedPrivateChat);
            request.invite = vec![user_id!("@alice:localhost").to_owned()];
            if carol_too {
                request.invite.push(user_id!("@carol:localhost").to_owned());
            }
            bob.create_room(request)
        };
        let dm = invite(None, true, false).await.unwrap();
        let group = invite(Some("Accept Test Group"), false, true)
            .await
            .unwrap();
        let declined = invite(Some("Decline Test"), false, false).await.unwrap();
        let [dm_id, group_id, declined_id] =
            [&dm, &group, &declined].map(|r| r.room_id().to_string());

        wait_for_buddies(&mut rx, "three invitations", |b| {
            [&dm_id, &group_id, &declined_id]
                .iter()
                .all(|id| section_of(b, id) == Some(Section::Invitations))
        })
        .await;

        rooms::accept(&alice, &dm_id).await.unwrap();
        rooms::accept(&alice, &group_id).await.unwrap();
        rooms::decline(&alice, &declined_id).await.unwrap();
        wait_for_buddies(
            &mut rx,
            "DM in Buddies, group in Chats, decline gone",
            |b| {
                section_of(b, &dm_id) == Some(Section::Buddies)
                    && section_of(b, &group_id) == Some(Section::Chats)
                    && section_of(b, &declined_id).is_none()
            },
        )
        .await;
        assert_eq!(
            rooms::accept(&alice, &declined_id).await.unwrap_err(),
            "That invitation is no longer open."
        );

        // Clean up: everyone leaves the test rooms.
        for id in [&dm_id, &group_id] {
            let room_id = matrix_sdk::ruma::RoomId::parse(id).unwrap();
            alice.get_room(&room_id).unwrap().leave().await.unwrap();
        }
        // Carol declines, so the invite doesn't pile up on her account. (Bob
        // can't withdraw it: the preset gives invitees his power level.)
        let carol_dir = tempfile::tempdir().unwrap();
        let carol = login(carol_dir.path(), PASS, HS, "carol", "aimtrix-test")
            .await
            .unwrap();
        carol
            .sync_once(matrix_sdk::config::SyncSettings::default())
            .await
            .unwrap();
        rooms::decline(&carol, &group_id).await.unwrap();
        carol.logout().await.unwrap();
        for room in [dm, group, declined] {
            room.leave().await.unwrap();
        }
        bob.logout().await.unwrap();
        watcher.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }

    /// Apply a timeline diff the way the IM window does.
    fn apply(lines: &mut Vec<timeline::Line>, diff: timeline::Diff) {
        use timeline::Diff::*;
        match diff {
            Append { values } => lines.extend(values),
            Clear => lines.clear(),
            PushFront { value } => lines.insert(0, value),
            PushBack { value } => lines.push(value),
            PopFront => {
                lines.remove(0);
            }
            PopBack => {
                lines.pop();
            }
            Insert { index, value } => lines.insert(index, value),
            Set { index, value } => lines[index] = value,
            Remove { index } => {
                lines.remove(index);
            }
            Truncate { length } => lines.truncate(length),
            Reset { values } => *lines = values,
        }
    }

    /// Apply diffs from `rx` until `want` holds; panic after 20 s.
    async fn wait_for_lines(
        lines: &mut Vec<timeline::Line>,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<timeline::Diff>,
        what: &str,
        want: impl Fn(&[timeline::Line]) -> bool,
    ) {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        while !want(lines) {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(diff)) => apply(lines, diff),
                _ => panic!("timed out waiting for {what}; lines: {lines:#?}"),
            }
        }
    }

    fn said(lines: &[timeline::Line], who: &str, text: &str, mine: bool) -> bool {
        lines.iter().any(|line| {
            matches!(line, timeline::Line::Message { name, body, own, .. }
                if name == who && body == text && *own == mine)
        })
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn timeline_shows_history_and_new_messages() {
        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = rooms::watch(syncing.room_list_service(), move |b| {
            let _ = tx.send(b);
        });
        let room_id = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        wait_for_buddies(&mut rx, "Bob's DM", {
            let room_id = room_id.clone();
            move |b| match find(b, "Bob Brown") {
                Some((_, bob)) => {
                    *room_id.lock().unwrap() = bob.room_id.clone();
                    true
                }
                None => false,
            }
        })
        .await;
        let room_id = room_id.lock().unwrap().clone();

        let (tx, mut diffs) = tokio::sync::mpsc::unbounded_channel();
        let (mut lines, task) = timeline::watch(
            &alice,
            &room_id,
            move |d| {
                let _ = tx.send(d);
            },
            |_| {},
        )
        .await
        .unwrap();
        // Earlier tests in this room leave at least one message from each.
        wait_for_lines(&mut lines, &mut diffs, "history from both", |l| {
            let from = |me: bool| {
                l.iter()
                    .any(|line| matches!(line, timeline::Line::Message { own, .. } if *own == me))
            };
            from(true) && from(false)
        })
        .await;
        // Profiles can load just after the lines, updating them.
        wait_for_lines(&mut lines, &mut diffs, "Bob's display name", |l| {
            l.iter().any(|line| {
                matches!(line,
                timeline::Line::Message { name, own: false, .. } if name == "Bob Brown")
            })
        })
        .await;

        let body = format!("timeline test {}", rand::random::<u32>());
        send_as("bob", &room_id, &body).await;
        wait_for_lines(&mut lines, &mut diffs, "Bob's new message", |l| {
            said(l, "Bob Brown", &body, false)
        })
        .await;
        assert!(
            matches!(lines.iter().rev().find(|l| matches!(l, timeline::Line::Message { .. })),
                Some(timeline::Line::Message { body: b, .. }) if *b == body),
            "the new message is the last one"
        );

        drop(task);
        watcher.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }

    /// Sign on as `user`, sync, and wait for a room on the list that `is_it`.
    /// Returns the client, its sync, the list watcher and the room ID.
    async fn sign_on_to_room(
        dir: &Path,
        user: &str,
        is_it: impl Fn(&rooms::Buddy) -> bool + Send + 'static,
    ) -> (
        Client,
        sync::Syncing,
        tauri::async_runtime::JoinHandle<()>,
        String,
    ) {
        let client = login(dir, PASS, HS, user, "aimtrix-test").await.unwrap();
        let syncing = sync::start(&client, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = rooms::watch(syncing.room_list_service(), move |b| {
            let _ = tx.send(b);
        });
        let room_id = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        wait_for_buddies(&mut rx, "the room", {
            let room_id = room_id.clone();
            move |b| match b.iter().find(|buddy| is_it(buddy)) {
                Some(buddy) => {
                    *room_id.lock().unwrap() = buddy.room_id.clone();
                    true
                }
                None => false,
            }
        })
        .await;
        let room_id = room_id.lock().unwrap().clone();
        (client, syncing, watcher, room_id)
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn sent_messages_show_at_once_and_reach_the_buddy() {
        let alice_dir = tempfile::tempdir().unwrap();
        let (alice, alice_sync, alice_list, room_id) =
            sign_on_to_room(alice_dir.path(), "alice", |b| b.name == "Bob Brown").await;
        let (tx, mut diffs) = tokio::sync::mpsc::unbounded_channel();
        let (mut lines, watching) = timeline::watch(
            &alice,
            &room_id,
            move |d| {
                let _ = tx.send(d);
            },
            |_| {},
        )
        .await
        .unwrap();

        let body = format!("send test {}\nsecond line", rand::random::<u32>());
        watching.send(&body, None).await.unwrap();
        wait_for_lines(&mut lines, &mut diffs, "Alice's own message", |l| {
            said(l, "Alice Anderson", &body, true)
        })
        .await;

        let bob_dir = tempfile::tempdir().unwrap();
        let (bob, bob_sync, bob_list, _) = sign_on_to_room(bob_dir.path(), "bob", {
            let room_id = room_id.clone();
            move |b| b.room_id == room_id
        })
        .await;
        let (tx, mut bob_diffs) = tokio::sync::mpsc::unbounded_channel();
        let (mut bob_lines, bob_watching) = timeline::watch(
            &bob,
            &room_id,
            move |d| {
                let _ = tx.send(d);
            },
            |_| {},
        )
        .await
        .unwrap();
        wait_for_lines(
            &mut bob_lines,
            &mut bob_diffs,
            "Alice's message at Bob's end",
            |l| said(l, "Alice Anderson", &body, false),
        )
        .await;
        assert!(
            !lines
                .iter()
                .any(|l| matches!(l, timeline::Line::Message { failed: true, .. })),
            "nothing failed to send"
        );

        for (watching, list, syncing, client) in [
            (watching, alice_list, alice_sync, alice),
            (bob_watching, bob_list, bob_sync, bob),
        ] {
            drop(watching);
            list.abort();
            syncing.stop().await;
            client.logout().await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn scrolling_back_reaches_the_first_message() {
        let dir = tempfile::tempdir().unwrap();
        let (alice, syncing, list, room_id) =
            sign_on_to_room(dir.path(), "alice", |b| b.name == "Test Chat").await;
        let (tx, mut diffs) = tokio::sync::mpsc::unbounded_channel();
        let (mut lines, watching) = timeline::watch(
            &alice,
            &room_id,
            move |d| {
                let _ = tx.send(d);
            },
            |_| {},
        )
        .await
        .unwrap();

        // Far more pages than the seeded backlog needs.
        let mut reached_start = false;
        for _ in 0..20 {
            if watching.load_older().await.unwrap() {
                reached_start = true;
                break;
            }
        }
        assert!(reached_start, "never reached the start of the room");
        wait_for_lines(
            &mut lines,
            &mut diffs,
            "the first message and the start",
            |l| {
                let first = l.iter().find(|line| {
                    matches!(
                        line,
                        timeline::Line::Message { .. } | timeline::Line::Start { .. }
                    )
                });
                matches!(first, Some(timeline::Line::Start { .. }))
                    && said(l, "Bob Brown", "backlog message 1", false)
            },
        )
        .await;
        // The backlog comes in order, oldest first.
        let order: Vec<_> = lines
            .iter()
            .filter_map(|line| match line {
                timeline::Line::Message { body, .. } => body
                    .strip_prefix("backlog message ")
                    .and_then(|n| n.parse::<u32>().ok()),
                _ => None,
            })
            .collect();
        assert_eq!(order, (1..=60).collect::<Vec<_>>());

        drop(watching);
        list.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }

    /// Wait up to 20 s for someone's typing list to equal `want`.
    async fn wait_for_typing(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<Vec<String>>,
        want: &[&str],
    ) {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(names)) if names == want => return,
                Ok(Some(_)) => {}
                _ => panic!("timed out waiting for typing {want:?}"),
            }
        }
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn typing_shows_both_ways() {
        let alice_dir = tempfile::tempdir().unwrap();
        let (alice, alice_sync, alice_list, room_id) =
            sign_on_to_room(alice_dir.path(), "alice", |b| b.name == "Bob Brown").await;
        let bob_dir = tempfile::tempdir().unwrap();
        let (bob, bob_sync, bob_list, _) = sign_on_to_room(bob_dir.path(), "bob", {
            let room_id = room_id.clone();
            move |b| b.room_id == room_id
        })
        .await;

        let (tx, mut alice_sees) = tokio::sync::mpsc::unbounded_channel();
        let (_, alice_watch) = timeline::watch(
            &alice,
            &room_id,
            |_| {},
            move |names| {
                let _ = tx.send(names);
            },
        )
        .await
        .unwrap();
        let (tx, mut bob_sees) = tokio::sync::mpsc::unbounded_channel();
        let (_, bob_watch) = timeline::watch(
            &bob,
            &room_id,
            |_| {},
            move |names| {
                let _ = tx.send(names);
            },
        )
        .await
        .unwrap();

        bob_watch.set_typing(true).await.unwrap();
        wait_for_typing(&mut alice_sees, &["Bob Brown"]).await;
        bob_watch.set_typing(false).await.unwrap();
        wait_for_typing(&mut alice_sees, &[]).await;

        alice_watch.set_typing(true).await.unwrap();
        wait_for_typing(&mut bob_sees, &["Alice Anderson"]).await;
        alice_watch.set_typing(false).await.unwrap();
        wait_for_typing(&mut bob_sees, &[]).await;

        for (watching, list, syncing, client) in [
            (alice_watch, alice_list, alice_sync, alice),
            (bob_watch, bob_list, bob_sync, bob),
        ] {
            drop(watching);
            list.abort();
            syncing.stop().await;
            client.logout().await.unwrap();
        }
    }

    /// Watch a room's lines; diffs go to the returned receiver.
    async fn watch_lines(
        client: &Client,
        room_id: &str,
    ) -> (
        Vec<timeline::Line>,
        timeline::Watching,
        tokio::sync::mpsc::UnboundedReceiver<timeline::Diff>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let (lines, watching) = timeline::watch(
            client,
            room_id,
            move |d| {
                let _ = tx.send(d);
            },
            |_| {},
        )
        .await
        .unwrap();
        (lines, watching, rx)
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn formatting_round_trips_and_scripts_are_stripped() {
        use matrix_sdk::ruma::{RoomId, events::room::message::RoomMessageEventContent};

        let alice_dir = tempfile::tempdir().unwrap();
        let (alice, alice_sync, alice_list, room_id) =
            sign_on_to_room(alice_dir.path(), "alice", |b| b.name == "Bob Brown").await;
        let bob_dir = tempfile::tempdir().unwrap();
        let (bob, bob_sync, bob_list, _) = sign_on_to_room(bob_dir.path(), "bob", {
            let room_id = room_id.clone();
            move |b| b.room_id == room_id
        })
        .await;
        let (mut alice_lines, alice_watch, mut alice_diffs) = watch_lines(&alice, &room_id).await;
        let (mut bob_lines, bob_watch, mut bob_diffs) = watch_lines(&bob, &room_id).await;

        // Alice formats; Bob gets the same formatting.
        let tag = rand::random::<u32>();
        let body = format!("bold red {tag}");
        let html = format!(r##"<b>bold</b> <font data-mx-color="#ff0000">red</font> {tag}"##);
        alice_watch.send(&body, Some(&html)).await.unwrap();
        // The SDK's own cleaner may rewrite the markup (e.g. `font` to
        // `span`), so check for the formatting, not exact HTML.
        wait_for_lines(&mut bob_lines, &mut bob_diffs, "Alice's formatting", |l| {
            l.iter().any(|line| {
                matches!(line, timeline::Line::Message { body: b, html: Some(h), .. }
                    if *b == body
                        && h.contains("<b>bold</b>")
                        && h.contains(r##"data-mx-color="#ff0000">red<"##))
            })
        })
        .await;

        // Another client sends a script; it never reaches Alice's window.
        let body = format!("hi {tag}");
        bob.get_room(&RoomId::parse(&room_id).unwrap())
            .unwrap()
            .send(RoomMessageEventContent::text_html(
                &body,
                format!("<script>alert(1)</script><b>hi</b> {tag}"),
            ))
            .await
            .unwrap();
        wait_for_lines(
            &mut alice_lines,
            &mut alice_diffs,
            "Bob's cleaned message",
            // The SDK's cleaner drops the tag but keeps its text, so check
            // that no script survives and the formatting does.
            |l| {
                l.iter().any(|line| {
                    matches!(line, timeline::Line::Message { body: b, html: Some(h), .. }
                        if *b == body && !h.contains("<script") && h.contains("<b>hi</b>"))
                })
            },
        )
        .await;

        for (watching, list, syncing, client) in [
            (alice_watch, alice_list, alice_sync, alice),
            (bob_watch, bob_list, bob_sync, bob),
        ] {
            drop(watching);
            list.abort();
            syncing.stop().await;
            client.logout().await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn reading_clears_the_bold() {
        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let list = rooms::watch(syncing.room_list_service(), move |b| {
            let _ = tx.send(b);
        });
        let room_id = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        wait_for_buddies(&mut rx, "Bob's DM", {
            let room_id = room_id.clone();
            move |b| match find(b, "Bob Brown") {
                Some((_, bob)) => {
                    *room_id.lock().unwrap() = bob.room_id.clone();
                    true
                }
                None => false,
            }
        })
        .await;
        let room_id = room_id.lock().unwrap().clone();
        let (mut lines, watching, mut diffs) = watch_lines(&alice, &room_id).await;
        let unread = |want: bool| {
            move |b: &[rooms::Buddy]| {
                find(b, "Bob Brown").is_some_and(|(_, bob)| bob.unread == want)
            }
        };

        // Start from read, whatever earlier tests left.
        watching.mark_read().await.unwrap();
        wait_for_buddies(&mut rx, "Bob read", unread(false)).await;

        let body = format!("read test {}", rand::random::<u32>());
        send_as("bob", &room_id, &body).await;
        wait_for_buddies(&mut rx, "Bob bold", unread(true)).await;
        // Reading marks up to the latest message, so wait until it's here.
        wait_for_lines(&mut lines, &mut diffs, "Bob's message", |l| {
            said(l, "Bob Brown", &body, false)
        })
        .await;
        watching.mark_read().await.unwrap();
        wait_for_buddies(&mut rx, "Bob read again", unread(false)).await;

        drop(watching);
        list.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn icon_pane_gets_bigger_icons() {
        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let list = rooms::watch(syncing.room_list_service(), move |b| {
            let _ = tx.send(b);
        });
        let buddies = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        wait_for_buddies(&mut rx, "Test Chat, Bob and Carol", {
            let buddies = buddies.clone();
            move |b| {
                *buddies.lock().unwrap() = b.to_vec();
                ["Test Chat", "Bob Brown", "Carol Chen"]
                    .iter()
                    .all(|name| find(b, name).is_some())
            }
        })
        .await;
        let buddies = buddies.lock().unwrap().clone();
        let icon = |name: &str| {
            let buddy = find(&buddies, name).unwrap().1.clone();
            let alice = alice.clone();
            async move { rooms::icon(&alice, &buddy, 96).await.unwrap() }
        };
        // Seeded avatars: Test Chat its own, Bob's DM his; Carol has none.
        for name in ["Test Chat", "Bob Brown"] {
            let icon = icon(name).await;
            assert!(
                icon.as_deref()
                    .is_some_and(|i| i.starts_with("data:image/png;base64,")),
                "{name}: {icon:?}"
            );
        }
        assert_eq!(icon("Carol Chen").await, None);

        let mine = alice.account().get_avatar_url().await.unwrap().unwrap();
        assert!(
            media::icon(&alice, &mine, 96)
                .await
                .unwrap()
                .starts_with("data:image/png;base64,")
        );

        list.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }
}
