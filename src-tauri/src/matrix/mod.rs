//! The single Matrix session owned by the Rust core: sign on, resume, sign off.

pub mod crypto;
mod html;
mod keyring;
pub mod media;
pub mod members;
pub mod presence;
pub mod rooms;
pub mod sync;
pub mod timeline;
pub mod verify;

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
    /// Who's in each open Chat Room, by room ID.
    pub members: std::sync::Mutex<HashMap<String, members::Watching>>,
    /// Whether this device can read old messages, for windows that open
    /// after it changed.
    pub recovery: std::sync::Mutex<crypto::Recovery>,
    /// Whether this device is verified, for windows that open after it
    /// changed.
    pub trust: std::sync::Mutex<verify::Trust>,
    /// The verification in progress, if any, and where it is.
    pub verification: std::sync::Mutex<Option<std::sync::Arc<verify::Flow>>>,
    pub verification_step: std::sync::Mutex<Option<verify::Step>>,
}

pub struct Signed {
    pub client: Client,
    pub sync: sync::Syncing,
    pub buddy_list: tauri::async_runtime::JoinHandle<()>,
    pub recovery: tauri::async_runtime::JoinHandle<()>,
    pub trust: tauri::async_runtime::JoinHandle<()>,
    pub verification_requests: matrix_sdk::event_handler::EventHandlerHandle,
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
    // Otherwise buddies see us online until the server times us out.
    presence::sign_off(&client).await;
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
        .with_encryption_settings(crypto::settings())
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
        .with_encryption_settings(crypto::settings())
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
    use base64::{Engine, engine::general_purpose::STANDARD};

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

    /// A DM's group: Buddies, or Online/Offline when the server has presence.
    fn is_buddy(section: rooms::Section) -> bool {
        use rooms::Section::*;
        matches!(section, Buddies | Online | Offline)
    }

    fn has_buddy(buddies: &[rooms::Buddy], name: &str) -> bool {
        buddies
            .iter()
            .any(|b| b.name == name && is_buddy(b.section))
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
            has_buddy(b, "Bob Brown")
                && has_buddy(b, "Carol Chen")
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
                section_of(b, &dm_id).is_some_and(is_buddy)
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

    /// Apply diffs from `rx` until `want` holds; panic after 30 s (room keys
    /// can take a while to arrive when the dev server is busy).
    async fn wait_for_lines(
        lines: &mut Vec<timeline::Line>,
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<timeline::Diff>,
        what: &str,
        want: impl Fn(&[timeline::Line]) -> bool,
    ) {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
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
        // By its seed marker: other tests make rooms that are also "Bob Brown".
        let _ = room_id;
        let room_id = seed_room(&alice, "dm-plain").await;

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

    /// The room `dev/seed.py` made under `key` (e.g. `dm-plain`), found by
    /// its `aimtrix.seed` marker. Names aren't enough: other tests make
    /// rooms that are also called "Bob Brown" for a while.
    async fn seed_room(client: &Client, key: &str) -> String {
        use matrix_sdk::ruma::{
            api::client::state::get_state_event_for_key::v3::Request, events::StateEventType,
        };
        for room in client.joined_rooms() {
            let request = Request::new(
                room.room_id().to_owned(),
                StateEventType::from("aimtrix.seed"),
                String::new(),
            );
            if let Ok(response) = client.send(request).await {
                let content: serde_json::Value =
                    serde_json::from_str(response.event_or_content.get()).unwrap_or_default();
                if content["key"] == key {
                    return room.room_id().to_string();
                }
            }
        }
        panic!("not in the seeded room {key}; run dev/seed.py");
    }

    /// [`sign_on_to_room`] for the seeded room `key`, whose Buddy List name
    /// is `name`.
    async fn sign_on_to_seed_room(
        dir: &Path,
        user: &str,
        key: &str,
        name: &str,
    ) -> (
        Client,
        sync::Syncing,
        tauri::async_runtime::JoinHandle<()>,
        String,
    ) {
        let name = name.to_owned();
        // Waiting for the name means the room list has loaded.
        let (client, syncing, watcher, _) =
            sign_on_to_room(dir, user, move |b| b.name == name).await;
        let room_id = seed_room(&client, key).await;
        syncing
            .room_list_service()
            .set_room_subscriptions(&[&matrix_sdk::ruma::RoomId::parse(&room_id).unwrap()])
            .await;
        (client, syncing, watcher, room_id)
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
        // Like an open IM window: sync sends this room in full.
        syncing
            .room_list_service()
            .set_room_subscriptions(&[&matrix_sdk::ruma::RoomId::parse(&room_id).unwrap()])
            .await;
        (client, syncing, watcher, room_id)
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn sent_messages_show_at_once_and_reach_the_buddy() {
        let alice_dir = tempfile::tempdir().unwrap();
        let (alice, alice_sync, alice_list, room_id) =
            sign_on_to_seed_room(alice_dir.path(), "alice", "dm-plain", "Bob Brown").await;
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
            sign_on_to_seed_room(dir.path(), "alice", "group-plain", "Test Chat").await;
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
        // The pages arrive as diffs, possibly still on their way, so wait
        // for the start and the whole backlog, oldest first.
        wait_for_lines(
            &mut lines,
            &mut diffs,
            "the start and the backlog in order",
            |l| {
                let first = l.iter().find(|line| {
                    matches!(
                        line,
                        timeline::Line::Message { .. } | timeline::Line::Start { .. }
                    )
                });
                let order: Vec<_> = l
                    .iter()
                    .filter_map(|line| match line {
                        timeline::Line::Message { body, .. } => body
                            .strip_prefix("backlog message ")
                            .and_then(|n| n.parse::<u32>().ok()),
                        _ => None,
                    })
                    .collect();
                matches!(first, Some(timeline::Line::Start { .. }))
                    && order == (1..=60).collect::<Vec<_>>()
            },
        )
        .await;

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
            sign_on_to_seed_room(alice_dir.path(), "alice", "dm-plain", "Bob Brown").await;
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
            sign_on_to_seed_room(alice_dir.path(), "alice", "dm-plain", "Bob Brown").await;
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
        // By its seed marker: other tests make rooms that are also "Bob Brown".
        let _ = room_id;
        let room_id = seed_room(&alice, "dm-plain").await;
        let (mut lines, watching, mut diffs) = watch_lines(&alice, &room_id).await;
        let unread = |want: bool| {
            let room_id = room_id.clone();
            move |b: &[rooms::Buddy]| {
                b.iter()
                    .any(|bob| bob.room_id == room_id && bob.unread == want)
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

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn images_files_edits_deletions_and_replies_show_up() {
        use matrix_sdk::{
            attachment::AttachmentConfig,
            ruma::{
                OwnedEventId, RoomId,
                events::{
                    relation::{InReplyTo, Reply},
                    room::message::{Relation, ReplacementMetadata, RoomMessageEventContent},
                },
            },
        };
        use timeline::{Line, MediaKind};

        let alice_dir = tempfile::tempdir().unwrap();
        let (alice, alice_sync, alice_list, room_id) =
            sign_on_to_seed_room(alice_dir.path(), "alice", "dm-plain", "Bob Brown").await;
        let (mut lines, watching, mut diffs) = watch_lines(&alice, &room_id).await;

        // Bob sends from another client.
        let bob_dir = tempfile::tempdir().unwrap();
        let bob = login(bob_dir.path(), PASS, HS, "bob", "aimtrix-test")
            .await
            .unwrap();
        bob.sync_once(matrix_sdk::config::SyncSettings::default())
            .await
            .unwrap();
        let room = bob.get_room(&RoomId::parse(&room_id).unwrap()).unwrap();
        let tag = rand::random::<u32>();
        // A 1x1 PNG.
        let png = STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
            .unwrap();
        let image_name = format!("cat-{tag}.png");
        room.send_attachment(&image_name, &mime::IMAGE_PNG, png, AttachmentConfig::new())
            .await
            .unwrap();
        let file_name = format!("notes-{tag}.txt");
        room.send_attachment(
            &file_name,
            &mime::TEXT_PLAIN,
            b"some notes".to_vec(),
            AttachmentConfig::new(),
        )
        .await
        .unwrap();
        let send = |content: RoomMessageEventContent| {
            let room = room.clone();
            async move { room.send(content).await.unwrap().response.event_id }
        };
        let original: OwnedEventId =
            send(RoomMessageEventContent::text_plain(format!("typo {tag}"))).await;
        send(
            RoomMessageEventContent::text_plain(format!("fixed {tag}"))
                .make_replacement(ReplacementMetadata::new(original.clone(), None)),
        )
        .await;
        let doomed = send(RoomMessageEventContent::text_plain(format!("oops {tag}"))).await;
        room.redact(&doomed, None, None).await.unwrap();
        let mut reply = RoomMessageEventContent::text_plain(format!("answer {tag}"));
        reply.relates_to = Some(Relation::Reply(Reply::new(InReplyTo::new(
            original.clone(),
        ))));
        send(reply).await;
        bob.logout().await.unwrap();

        let image = |l: &[Line]| {
            l.iter().find_map(|line| match line {
                Line::Message {
                    media: Some(m),
                    event_id: Some(id),
                    ..
                } if m.kind == MediaKind::Image && m.name == image_name => Some(id.clone()),
                _ => None,
            })
        };
        wait_for_lines(&mut lines, &mut diffs, "every kind of message", |l| {
            let file = l.iter().any(|line| {
                matches!(line, Line::Message { media: Some(m), .. }
                    if m.kind == MediaKind::File && m.name == file_name)
            });
            let edited = l.iter().any(|line| {
                matches!(line, Line::Message { body, edited: true, .. } if *body == format!("fixed {tag}"))
            });
            let deleted = l.iter().any(|line| matches!(line, Line::Deleted { name, .. } if name == "Bob Brown"));
            let replied = l.iter().any(|line| {
                matches!(line, Line::Message { body, reply: Some(r), .. }
                    if *body == format!("answer {tag}")
                        && r.name.as_deref() == Some("Bob Brown")
                        && r.preview.as_deref() == Some(format!("fixed {tag}").as_str()))
            });
            image(l).is_some() && file && edited && deleted && replied
        })
        .await;
        // The old version is gone, and so is the deleted text.
        assert!(
            !lines
                .iter()
                .any(|line| matches!(line, Line::Message { body, .. }
            if *body == format!("typo {tag}") || *body == format!("oops {tag}")))
        );

        let thumbnail = watching.thumbnail(&image(&lines).unwrap()).await.unwrap();
        assert!(
            thumbnail.starts_with("data:image/"),
            "{}",
            &thumbnail[..30.min(thumbnail.len())]
        );

        drop(watching);
        alice_list.abort();
        alice_sync.stop().await;
        alice.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn replies_to_old_messages_load_their_preview() {
        use matrix_sdk::{
            room::MessagesOptions,
            ruma::{
                RoomId,
                events::{
                    AnySyncMessageLikeEvent, AnySyncTimelineEvent, SyncMessageLikeEvent,
                    relation::{InReplyTo, Reply},
                    room::message::{Relation, RoomMessageEventContent},
                },
                uint,
            },
        };
        use timeline::Line;

        let alice_dir = tempfile::tempdir().unwrap();
        let (alice, alice_sync, alice_list, room_id) =
            sign_on_to_seed_room(alice_dir.path(), "alice", "group-plain", "Test Chat").await;

        // Bob replies to the first backlog message, far older than the
        // first page a window loads.
        let bob_dir = tempfile::tempdir().unwrap();
        let bob = login(bob_dir.path(), PASS, HS, "bob", "aimtrix-test")
            .await
            .unwrap();
        bob.sync_once(matrix_sdk::config::SyncSettings::default())
            .await
            .unwrap();
        let room = bob.get_room(&RoomId::parse(&room_id).unwrap()).unwrap();
        let mut options = MessagesOptions::backward();
        options.limit = uint!(200);
        let first = room
            .messages(options)
            .await
            .unwrap()
            .chunk
            .into_iter()
            .find_map(|event| match event.raw().deserialize() {
                Ok(AnySyncTimelineEvent::MessageLike(AnySyncMessageLikeEvent::RoomMessage(
                    SyncMessageLikeEvent::Original(m),
                ))) if m.content.body() == "backlog message 1" => Some(m.event_id),
                _ => None,
            })
            .expect("the seeded backlog");
        let body = format!("old reply {}", rand::random::<u32>());
        let mut reply = RoomMessageEventContent::text_plain(&body);
        reply.relates_to = Some(Relation::Reply(Reply::new(InReplyTo::new(first))));
        room.send(reply).await.unwrap();
        bob.logout().await.unwrap();

        let (mut lines, watching, mut diffs) = watch_lines(&alice, &room_id).await;
        wait_for_lines(&mut lines, &mut diffs, "the reply's preview", |l| {
            l.iter().any(|line| {
                matches!(line, Line::Message { body: b, reply: Some(r), .. }
                    if *b == body
                        && r.name.as_deref() == Some("Bob Brown")
                        && r.preview.as_deref() == Some("backlog message 1"))
            })
        })
        .await;

        drop(watching);
        alice_list.abort();
        alice_sync.stop().await;
        alice.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn attachments_save_with_their_contents() {
        use matrix_sdk::{attachment::AttachmentConfig, ruma::RoomId};
        use timeline::{Line, MediaKind};

        let alice_dir = tempfile::tempdir().unwrap();
        let (alice, alice_sync, alice_list, room_id) =
            sign_on_to_seed_room(alice_dir.path(), "alice", "dm-plain", "Bob Brown").await;
        let (mut lines, watching, mut diffs) = watch_lines(&alice, &room_id).await;

        let tag = rand::random::<u32>();
        let text = format!("notes for {tag}\nsecond line\n").into_bytes();
        let png = STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
            .unwrap();
        // A hostile name: saving must not leave the chosen folder.
        let file_name = format!("../../save-{tag}.txt");
        let image_name = format!("pic-{tag}.png");
        {
            let bob_dir = tempfile::tempdir().unwrap();
            let bob = login(bob_dir.path(), PASS, HS, "bob", "aimtrix-test")
                .await
                .unwrap();
            bob.sync_once(matrix_sdk::config::SyncSettings::default())
                .await
                .unwrap();
            let room = bob.get_room(&RoomId::parse(&room_id).unwrap()).unwrap();
            room.send_attachment(
                &file_name,
                &mime::TEXT_PLAIN,
                text.clone(),
                AttachmentConfig::new(),
            )
            .await
            .unwrap();
            room.send_attachment(
                &image_name,
                &mime::IMAGE_PNG,
                png.clone(),
                AttachmentConfig::new(),
            )
            .await
            .unwrap();
            bob.logout().await.unwrap();
        }

        let event_of = |l: &[Line], kind: MediaKind, name: &str| {
            l.iter().find_map(|line| match line {
                Line::Message {
                    media: Some(m),
                    event_id: Some(id),
                    ..
                } if m.kind == kind && m.name == name => Some(id.clone()),
                _ => None,
            })
        };
        wait_for_lines(&mut lines, &mut diffs, "both attachments", |l| {
            event_of(l, MediaKind::File, &file_name).is_some()
                && event_of(l, MediaKind::Image, &image_name).is_some()
        })
        .await;

        let out = tempfile::tempdir().unwrap();
        for (kind, name, saved_name, contents) in [
            (
                MediaKind::File,
                &file_name,
                format!("save-{tag}.txt"),
                &text,
            ),
            (MediaKind::Image, &image_name, image_name.clone(), &png),
        ] {
            let event_id = event_of(&lines, kind, name).unwrap();
            assert_eq!(
                watching.attachment_name(&event_id).await.unwrap(),
                saved_name
            );
            let path = out.path().join(&saved_name);
            watching
                .save_attachment(&event_id, path.clone())
                .await
                .unwrap();
            assert_eq!(&std::fs::read(&path).unwrap(), contents, "{saved_name}");
        }

        drop(watching);
        alice_list.abort();
        alice_sync.stop().await;
        alice.logout().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn chat_room_follows_members_topic_and_room_events() {
        use matrix_sdk::ruma::{
            api::client::room::create_room::v3::{Request, RoomPreset},
            user_id,
        };
        use timeline::{Line, MemberChange};

        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let mut request = Request::new();
        request.name = Some("Members Test".to_owned());
        request.preset = Some(RoomPreset::PrivateChat);
        request.invite = vec![user_id!("@bob:localhost").to_owned()];
        let room = alice.create_room(request).await.unwrap();
        let room_id = room.room_id().to_owned();
        // Like an open Chat Room: sync sends this room in full.
        syncing
            .room_list_service()
            .set_room_subscriptions(&[&room_id])
            .await;

        let (tx, mut member_changes) = tokio::sync::mpsc::unbounded_channel();
        let (topic_tx, mut topic_changes) = tokio::sync::mpsc::unbounded_channel();
        let (first, watching) = members::watch(
            &alice,
            room_id.as_str(),
            move |m| {
                let _ = tx.send(m);
            },
            move |t| {
                let _ = topic_tx.send(t);
            },
        )
        .await
        .unwrap();
        let (mut lines, timeline, mut diffs) = watch_lines(&alice, room_id.as_str()).await;
        let names = |m: &[members::Member]| m.iter().map(|m| m.name.clone()).collect::<Vec<_>>();
        assert_eq!(names(&first.members), ["Alice Anderson"]);
        assert!(first.members[0].own);
        assert_eq!(first.topic, None);

        let wait_for_members = async |rx: &mut tokio::sync::mpsc::UnboundedReceiver<_>,
                                      count: usize| {
            tokio::time::timeout(std::time::Duration::from_secs(30), async {
                loop {
                    let members: Vec<members::Member> = rx.recv().await.expect("watch stopped");
                    if members.len() == count {
                        break members;
                    }
                }
            })
            .await
            .unwrap_or_else(|_| panic!("the member list never had {count} people"))
        };
        let has = |lines: &[Line], who: &str, what: MemberChange| {
            lines.iter().any(|l| {
                matches!(l, Line::Membership { name, change, .. } if name == who && *change == what)
            })
        };

        let bob_dir = tempfile::tempdir().unwrap();
        let bob = login(bob_dir.path(), PASS, HS, "bob", "aimtrix-test")
            .await
            .unwrap();
        bob.join_room_by_id(&room_id).await.unwrap();
        let joined = wait_for_members(&mut member_changes, 2).await;
        assert_eq!(names(&joined), ["Alice Anderson", "Bob Brown"]);
        assert!(!joined[1].own);
        wait_for_lines(&mut lines, &mut diffs, "Bob entering", |l| {
            has(l, "Bob Brown", MemberChange::Joined)
        })
        .await;
        assert!(has(&lines, "Bob Brown", MemberChange::Invited));

        room.set_room_topic("  test topic  ").await.unwrap();
        let topic = tokio::time::timeout(std::time::Duration::from_secs(30), topic_changes.recv())
            .await
            .expect("the topic never changed");
        assert_eq!(topic, Some(Some("test topic".to_owned())));
        wait_for_lines(&mut lines, &mut diffs, "the topic line", |l| {
            l.iter().any(|l| {
                matches!(l, Line::Topic { name, topic: Some(t), .. }
                    if name == "Alice Anderson" && t == "test topic")
            })
        })
        .await;

        bob.get_room(&room_id).unwrap().leave().await.unwrap();
        wait_for_members(&mut member_changes, 1).await;
        wait_for_lines(&mut lines, &mut diffs, "Bob leaving", |l| {
            has(l, "Bob Brown", MemberChange::Left)
        })
        .await;

        // Clean up: Alice leaves too.
        bob.logout().await.unwrap();
        drop(timeline);
        drop(watching);
        room.leave().await.unwrap();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }

    /// Wait until `sender` knows `receiver`'s device, so a message it
    /// encrypts now includes the key for it. A brand-new device reaches
    /// other users' device lists through sync, which can lag under load.
    async fn knows_device(sender: &Client, receiver: &Client) {
        let (user, device) = (receiver.user_id().unwrap(), receiver.device_id().unwrap());
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        while sender
            .encryption()
            .get_device(user, device)
            .await
            .unwrap()
            .is_none()
        {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {} to know {user}'s new device",
                sender.user_id().unwrap()
            );
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn encrypted_rooms_send_and_receive_both_ways() {
        // Room, Alice's name for it, the other member and their name for it.
        for (key, name, other, other_name, other_display) in [
            (
                "dm-e2ee",
                "Carol Chen",
                "carol",
                "Alice Anderson",
                "Carol Chen",
            ),
            (
                "group-e2ee",
                "Secret Chat",
                "bob",
                "Secret Chat",
                "Bob Brown",
            ),
        ] {
            let alice_dir = tempfile::tempdir().unwrap();
            let (alice, alice_sync, alice_list, room_id) =
                sign_on_to_seed_room(alice_dir.path(), "alice", key, name).await;
            let other_dir = tempfile::tempdir().unwrap();
            let (them, their_sync, their_list, _) =
                sign_on_to_seed_room(other_dir.path(), other, key, other_name).await;
            let room = alice
                .get_room(&matrix_sdk::ruma::RoomId::parse(&room_id).unwrap())
                .unwrap();
            assert!(
                room.latest_encryption_state().await.unwrap().is_encrypted(),
                "{key} is encrypted"
            );

            let (mut alice_lines, alice_watch, mut alice_diffs) =
                watch_lines(&alice, &room_id).await;
            let (mut their_lines, their_watch, mut their_diffs) =
                watch_lines(&them, &room_id).await;

            let body = format!("e2ee from alice {}", rand::random::<u32>());
            knows_device(&alice, &them).await;
            alice_watch.send(&body, None).await.unwrap();
            wait_for_lines(&mut their_lines, &mut their_diffs, "Alice's message", |l| {
                said(l, "Alice Anderson", &body, false)
            })
            .await;

            let body = format!("e2ee from {other} {}", rand::random::<u32>());
            knows_device(&them, &alice).await;
            their_watch.send(&body, None).await.unwrap();
            wait_for_lines(&mut alice_lines, &mut alice_diffs, "the reply", |l| {
                said(l, other_display, &body, false)
            })
            .await;

            for (watching, list, syncing, client) in [
                (alice_watch, alice_list, alice_sync, alice),
                (their_watch, their_list, their_sync, them),
            ] {
                drop(watching);
                list.abort();
                syncing.stop().await;
                client.logout().await.unwrap();
            }
        }
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn undecryptable_messages_turn_into_text_when_keys_arrive() {
        // Alice's first device gets the key as Carol sends.
        let first_dir = tempfile::tempdir().unwrap();
        let (first, first_sync, first_list, room_id) =
            sign_on_to_seed_room(first_dir.path(), "alice", "dm-e2ee", "Carol Chen").await;
        let (mut first_lines, first_watch, mut first_diffs) = watch_lines(&first, &room_id).await;
        let carol_dir = tempfile::tempdir().unwrap();
        let (carol, carol_sync, carol_list, _) =
            sign_on_to_seed_room(carol_dir.path(), "carol", "dm-e2ee", "Alice Anderson").await;
        let (_, carol_watch, _) = watch_lines(&carol, &room_id).await;
        let body = format!("utd test {}", rand::random::<u32>());
        knows_device(&carol, &first).await;
        carol_watch.send(&body, None).await.unwrap();
        wait_for_lines(&mut first_lines, &mut first_diffs, "Carol's message", |l| {
            said(l, "Carol Chen", &body, false)
        })
        .await;
        let event_id = first_lines
            .iter()
            .find_map(|l| match l {
                timeline::Line::Message {
                    event_id, body: b, ..
                } if *b == body => event_id.clone(),
                _ => None,
            })
            .unwrap();

        // A second device, signed on later, has no key for it…
        let second_dir = tempfile::tempdir().unwrap();
        let (second, second_sync, second_list, _) =
            sign_on_to_seed_room(second_dir.path(), "alice", "dm-e2ee", "Carol Chen").await;
        let (mut lines, second_watch, mut diffs) = watch_lines(&second, &room_id).await;
        wait_for_lines(&mut lines, &mut diffs, "the placeholder", |l| {
            l.iter().any(|line| {
                matches!(line, timeline::Line::Undecryptable { event_id: Some(e), name, .. }
                    if *e == event_id && name == "Carol Chen")
            })
        })
        .await;

        // …until it gets one (here by key export; 5.3 brings key backup).
        let keys = first_dir.path().join("keys.txt");
        let room = matrix_sdk::ruma::RoomId::parse(&room_id).unwrap();
        first
            .encryption()
            .export_room_keys(keys.clone(), "export pass", |s| s.room_id() == room)
            .await
            .unwrap();
        second
            .encryption()
            .import_room_keys(keys, "export pass")
            .await
            .unwrap();
        wait_for_lines(&mut lines, &mut diffs, "the decrypted message", |l| {
            said(l, "Carol Chen", &body, false)
        })
        .await;

        for (watching, list, syncing, client) in [
            (first_watch, first_list, first_sync, first),
            (second_watch, second_list, second_sync, second),
            (carol_watch, carol_list, carol_sync, carol),
        ] {
            drop(watching);
            list.abort();
            syncing.stop().await;
            client.logout().await.unwrap();
        }
    }

    /// Wait for `client`'s recovery state to be `want`; panic after 20 s.
    async fn wait_for_recovery(client: &Client, what: &str, want: crypto::Recovery) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let task = crypto::watch(client, move |state| {
            let _ = tx.send(state);
        });
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut last = None;
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(state)) if state == want => break,
                Ok(Some(state)) => last = Some(state),
                _ => panic!(
                    "timed out waiting for {what} to be {want:?}; last {last:?}; cross-signing {:?}, backups enabled {:?} ({:?})",
                    client.encryption().cross_signing_status().await,
                    client.encryption().backups().are_enabled().await,
                    client.encryption().backups().state(),
                ),
            }
        }
        task.abort();
    }

    /// Bob's recovery passphrase, set up by the first run of
    /// [`recovery_key_lets_a_new_device_read_old_messages`] and kept after.
    const BOB_RECOVERY: &str = "bob test recovery passphrase";

    /// Make sure bob's account has recovery with [`BOB_RECOVERY`], setting
    /// it up (with a new identity, since recovery needs cross-signing) if it
    /// has none or an earlier run left it half done.
    ///
    /// Runs on its own sign-on with no sync: while syncing, the SDK can see
    /// the old identity on the server between making new cross-signing keys
    /// and uploading them, and throws the new private keys away.
    async fn ensure_bob_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let client = login(dir.path(), PASS, HS, "bob", "aimtrix-test")
            .await
            .unwrap();
        client
            .encryption()
            .wait_for_e2ee_initialization_tasks()
            .await;
        let recovered = crypto::recover(&client, BOB_RECOVERY).await.is_ok()
            && has_cross_signing(&client).await;
        if !recovered {
            let recovery = client.encryption().recovery();
            if let Some(reset) = recovery.reset_identity().await.unwrap() {
                let matrix_sdk::encryption::CrossSigningResetAuthType::Uiaa(uiaa) =
                    reset.auth_type()
                else {
                    panic!("the dev homeserver asks for a password");
                };
                use matrix_sdk::ruma::api::client::uiaa::{
                    AuthData, MatrixUserIdentifier, Password,
                };
                let mut password = Password::new(
                    MatrixUserIdentifier::new("bob".to_owned()).into(),
                    "aimtrix-test".to_owned(),
                );
                password.session = uiaa.session.clone();
                reset
                    .reset(Some(AuthData::Password(password)))
                    .await
                    .unwrap();
            }
            assert!(
                has_cross_signing(&client).await,
                "the reset kept the cross-signing keys"
            );
            recovery
                .enable()
                .with_passphrase(BOB_RECOVERY)
                .await
                .unwrap();
        }
        client.logout().await.unwrap();
    }

    /// Whether `client` holds all of its account's private cross-signing keys.
    async fn has_cross_signing(client: &Client) -> bool {
        client
            .encryption()
            .cross_signing_status()
            .await
            .is_some_and(|s| s.is_complete())
    }

    /// Uses bob, not alice, so it can't break a recovery key made for alice
    /// in Element. The first run sets up bob's recovery with
    /// [`BOB_RECOVERY`] (see [`ensure_bob_recovery`]); later runs reuse it.
    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn recovery_key_lets_a_new_device_read_old_messages() {
        // Bob's first device has recovery and backs up a message's key.
        ensure_bob_recovery().await;
        let first_dir = tempfile::tempdir().unwrap();
        let (first, first_sync, first_list, room_id) =
            sign_on_to_seed_room(first_dir.path(), "bob", "group-e2ee", "Secret Chat").await;
        wait_for_recovery(
            &first,
            "recovery on the first device",
            crypto::Recovery::Incomplete,
        )
        .await;
        crypto::recover(&first, BOB_RECOVERY).await.unwrap();
        let (mut first_lines, first_watch, mut first_diffs) = watch_lines(&first, &room_id).await;
        let body = format!("recovery test {}", rand::random::<u32>());
        send_as("alice", &room_id, &body).await;
        wait_for_lines(&mut first_lines, &mut first_diffs, "Alice's message", |l| {
            said(l, "Alice Anderson", &body, false)
        })
        .await;
        first
            .encryption()
            .backups()
            .wait_for_steady_state()
            .await
            .unwrap();

        // A new device can't read it…
        let second_dir = tempfile::tempdir().unwrap();
        let (second, second_sync, second_list, _) =
            sign_on_to_seed_room(second_dir.path(), "bob", "group-e2ee", "Secret Chat").await;
        let (mut lines, second_watch, mut diffs) = watch_lines(&second, &room_id).await;
        wait_for_lines(&mut lines, &mut diffs, "the placeholder", |l| {
            l.iter().any(|line| {
                matches!(line, timeline::Line::Undecryptable { name, .. } if name == "Alice Anderson")
            }) && !said(l, "Alice Anderson", &body, false)
        })
        .await;
        wait_for_recovery(
            &second,
            "recovery before the key",
            crypto::Recovery::Incomplete,
        )
        .await;

        // …until it's given the passphrase, pasted with stray whitespace.
        assert_eq!(
            crypto::recover(
                &second,
                "EsTj 3yST y93F SLpB jJsz eAXc 2XzA ygD3 w69H fGaN TKBj jXEd"
            )
            .await,
            Err("That recovery key or passphrase isn't right.".to_owned())
        );
        crypto::recover(&second, &format!("  {BOB_RECOVERY}\n"))
            .await
            .unwrap();
        wait_for_recovery(&second, "recovery after the key", crypto::Recovery::Enabled).await;
        wait_for_lines(&mut lines, &mut diffs, "the restored message", |l| {
            said(l, "Alice Anderson", &body, false)
        })
        .await;

        for (watching, list, syncing, client) in [
            (first_watch, first_list, first_sync, first),
            (second_watch, second_list, second_sync, second),
        ] {
            drop(watching);
            list.abort();
            syncing.stop().await;
            client.logout().await.unwrap();
        }
    }

    /// Wait for a step `is_it` from `rx`, skipping others; panic after 30 s.
    async fn wait_for_step(
        rx: &mut tokio::sync::mpsc::UnboundedReceiver<verify::Step>,
        what: &str,
        is_it: impl Fn(&verify::Step) -> bool,
    ) -> verify::Step {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut last = None;
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(step)) if is_it(&step) => return step,
                Ok(Some(step)) => last = Some(step),
                _ => panic!("timed out waiting for {what}; last step {last:?}"),
            }
        }
    }

    /// Wait for `client`'s device trust to be `want`; panic after 30 s.
    async fn wait_for_trust(client: &Client, what: &str, want: verify::Trust) {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let task = verify::watch_trust(client, move |trust| {
            let _ = tx.send(trust);
        });
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut last = None;
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(trust)) if trust == want => break,
                Ok(Some(trust)) => last = Some(trust),
                _ => panic!("timed out waiting for {what} to be {want:?}; last {last:?}"),
            }
        }
        task.abort();
    }

    /// Uses bob, for the cross-signing [`ensure_bob_recovery`] sets up.
    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn emoji_verification_makes_a_new_device_trusted() {
        // Bob's first device is verified (recovery brings the identity's
        // keys) and listens for requests.
        ensure_bob_recovery().await;
        let first_dir = tempfile::tempdir().unwrap();
        let (first, first_sync, first_list, _) =
            sign_on_to_seed_room(first_dir.path(), "bob", "group-e2ee", "Secret Chat").await;
        wait_for_recovery(
            &first,
            "recovery on the first device",
            crypto::Recovery::Incomplete,
        )
        .await;
        crypto::recover(&first, BOB_RECOVERY).await.unwrap();
        wait_for_trust(&first, "the first device", verify::Trust::Verified).await;
        let (tx, mut requests) = tokio::sync::mpsc::unbounded_channel();
        let handler = verify::on_requests(&first, move |request| {
            let _ = tx.send(request);
        });

        // A new device isn't, and asks to be.
        let second_dir = tempfile::tempdir().unwrap();
        let (second, second_sync, second_list, _) =
            sign_on_to_seed_room(second_dir.path(), "bob", "group-e2ee", "Secret Chat").await;
        wait_for_trust(&second, "the new device", verify::Trust::Unverified).await;
        let (tx, mut second_steps) = tokio::sync::mpsc::unbounded_channel();
        let second_flow = verify::Flow::request(&second, move |step| {
            let _ = tx.send(step);
        })
        .await
        .unwrap();
        wait_for_step(&mut second_steps, "the request to go out", |s| {
            *s == verify::Step::Waiting
        })
        .await;

        // The first device accepts, and both show the same emoji.
        let request = tokio::time::timeout(std::time::Duration::from_secs(30), requests.recv())
            .await
            .expect("the first device gets the request")
            .unwrap();
        let (tx, mut first_steps) = tokio::sync::mpsc::unbounded_channel();
        let first_flow = verify::Flow::incoming(request, move |step| {
            let _ = tx.send(step);
        });
        wait_for_step(
            &mut first_steps,
            "the incoming request",
            |s| matches!(s, verify::Step::Incoming { device } if device == "AIMtrix"),
        )
        .await;
        first_flow.accept().await.unwrap();
        let is_compare = |s: &verify::Step| matches!(s, verify::Step::Compare { .. });
        let first_emojis = wait_for_step(&mut first_steps, "emoji on the first", is_compare).await;
        let second_emojis =
            wait_for_step(&mut second_steps, "emoji on the second", is_compare).await;
        let emojis = |step: verify::Step| match step {
            verify::Step::Compare { emojis, .. } => emojis,
            _ => unreachable!(),
        };
        let first_emojis = emojis(first_emojis);
        assert_eq!(first_emojis.len(), 7);
        assert_eq!(first_emojis, emojis(second_emojis));

        // Both say they match: done, and the new device is now trusted.
        second_flow.confirm(true).await.unwrap();
        first_flow.confirm(true).await.unwrap();
        let is_done = |s: &verify::Step| matches!(s, verify::Step::Done { .. });
        wait_for_step(&mut first_steps, "done on the first", is_done).await;
        wait_for_step(&mut second_steps, "done on the second", is_done).await;
        wait_for_trust(&second, "the new device", verify::Trust::Verified).await;

        first.remove_event_handler(handler);
        drop((first_flow, second_flow));
        for (list, syncing, client) in [
            (first_list, first_sync, first),
            (second_list, second_sync, second),
        ] {
            list.abort();
            syncing.stop().await;
            client.logout().await.unwrap();
        }
    }

    /// Register a new user with the dev server's shared secret (see
    /// `dev/seed.py`), password `aimtrix-test`. Deactivate it when done.
    async fn register_throwaway_user(prefix: &str) -> String {
        use hmac::{Hmac, Mac};
        const ADMIN: &str = "http://localhost:8008/_synapse/admin/v1/register";
        let user = format!("{prefix}-{}", rand::random::<u32>());
        let http = matrix_sdk::reqwest::Client::new();
        let get: serde_json::Value =
            serde_json::from_str(&http.get(ADMIN).send().await.unwrap().text().await.unwrap())
                .unwrap();
        let nonce = get["nonce"].as_str().unwrap();
        let mut mac = Hmac::<sha1::Sha1>::new_from_slice(b"aimtrix-dev-secret").unwrap();
        mac.update(format!("{nonce}\0{user}\0aimtrix-test\0notadmin").as_bytes());
        let body = serde_json::json!({
            "nonce": nonce, "username": user, "password": "aimtrix-test", "admin": false,
            "mac": hex::encode(mac.finalize().into_bytes()),
        });
        let response = http
            .post(ADMIN)
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "couldn't register {user}");
        user
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn new_accounts_get_a_recovery_key_that_restores_elsewhere() {
        let user = register_throwaway_user("setup").await;
        // A new account gets a cross-signing identity at sign-on, so this
        // device can set up recovery right away.
        let first_dir = tempfile::tempdir().unwrap();
        let first = login(first_dir.path(), PASS, HS, &user, "aimtrix-test")
            .await
            .unwrap();
        let first_sync = sync::start(&first, |_| {}).await.unwrap();
        wait_for_trust(&first, "the first device", verify::Trust::Verified).await;
        wait_for_recovery(&first, "recovery before setup", crypto::Recovery::Disabled).await;
        assert_eq!(crypto::setup_check(&first).await, Ok(crypto::Setup::Ready));
        // A second device can't set it up: it doesn't have the identity's
        // keys, so it has to verify first.
        let second_dir = tempfile::tempdir().unwrap();
        let second = login(second_dir.path(), PASS, HS, &user, "aimtrix-test")
            .await
            .unwrap();
        let second_sync = sync::start(&second, |_| {}).await.unwrap();
        wait_for_trust(&second, "the second device", verify::Trust::Unverified).await;
        assert_eq!(
            crypto::setup_check(&second).await,
            Ok(crypto::Setup::VerifyFirst)
        );

        let recovery_key = crypto::set_up(&first).await.unwrap();
        wait_for_recovery(&first, "recovery after setup", crypto::Recovery::Enabled).await;
        assert_eq!(
            crypto::setup_check(&first).await,
            Ok(crypto::Setup::AlreadySetUp)
        );

        // The key restores the second device: identity and backup.
        wait_for_recovery(
            &second,
            "recovery on the second",
            crypto::Recovery::Incomplete,
        )
        .await;
        crypto::recover(&second, &recovery_key).await.unwrap();
        wait_for_recovery(&second, "recovery on the second", crypto::Recovery::Enabled).await;
        wait_for_trust(&second, "the second device", verify::Trust::Verified).await;
        assert!(second.encryption().backups().are_enabled().await);

        second_sync.stop().await;
        second.logout().await.unwrap();
        first_sync.stop().await;
        use matrix_sdk::ruma::api::client::uiaa::{AuthData, MatrixUserIdentifier, Password};
        let password = Password::new(
            MatrixUserIdentifier::new(user.clone()).into(),
            "aimtrix-test".to_owned(),
        );
        // Signs out every device too.
        first
            .account()
            .deactivate(None, Some(AuthData::Password(password)), true)
            .await
            .unwrap();
    }

    /// The second dev homeserver, with presence off (the `no-presence`
    /// compose profile).
    const NO_PRESENCE_HS: &str = "http://localhost:8009";

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn presence_splits_buddies_into_online_and_offline() {
        use matrix_sdk::ruma::{api::client::room::create_room::v3::Request, user_id};
        use rooms::Section;
        use std::time::Duration;

        // A fresh buddy, since other tests keep the seeded users online.
        let user = register_throwaway_user("presence").await;
        let buddy_dir = tempfile::tempdir().unwrap();
        let buddy = login(buddy_dir.path(), PASS, HS, &user, "aimtrix-test")
            .await
            .unwrap();
        let mut request = Request::new();
        request.is_direct = true;
        request.invite = vec![user_id!("@alice:localhost").to_owned()];
        let room = buddy.create_room(request).await.unwrap();

        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, HS, "alice", "aimtrix-test")
            .await
            .unwrap();
        alice.join_room_by_id(room.room_id()).await.unwrap();
        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = rooms::watch_every(
            syncing.room_list_service(),
            Duration::from_secs(1),
            move |b| {
                let _ = tx.send(b);
            },
        );
        let room_id = room.room_id().to_string();
        let section_of = move |b: &[rooms::Buddy]| {
            b.iter()
                .find(|b| b.room_id == room_id)
                .map(|b| (b.section, b.presence))
        };

        wait_for_buddies(&mut rx, "the buddy offline", |b| {
            section_of(b) == Some((Section::Offline, Some(presence::Presence::Offline)))
        })
        .await;
        let (changed, _) = tokio::sync::mpsc::unbounded_channel();
        let tracker = presence::Tracker::new(buddy.clone(), Duration::from_secs(1), changed);
        wait_for_buddies(&mut rx, "the buddy online", |b| {
            section_of(b) == Some((Section::Online, Some(presence::Presence::Online)))
        })
        .await;
        drop(tracker);
        presence::sign_off(&buddy).await;
        wait_for_buddies(&mut rx, "the buddy offline again", |b| {
            section_of(b) == Some((Section::Offline, Some(presence::Presence::Offline)))
        })
        .await;

        watcher.abort();
        syncing.stop().await;
        if let Some(dm) = alice.get_room(room.room_id()) {
            dm.leave().await.unwrap();
        }
        alice.logout().await.unwrap();
        use matrix_sdk::ruma::api::client::uiaa::{AuthData, MatrixUserIdentifier, Password};
        let password = Password::new(
            MatrixUserIdentifier::new(user.clone()).into(),
            "aimtrix-test".to_owned(),
        );
        buddy
            .account()
            .deactivate(None, Some(AuthData::Password(password)), true)
            .await
            .unwrap();
    }

    #[tokio::test]
    #[ignore = "needs the dev homeserver"]
    async fn without_presence_buddies_stay_in_one_group() {
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let alice = login(dir.path(), PASS, NO_PRESENCE_HS, "alice", "aimtrix-test")
            .await
            .unwrap_or_else(|e| {
                panic!("{e} (start it with `docker compose -f dev/docker-compose.yml --profile no-presence up -d`)")
            });
        // Find out the way the Buddy List does.
        let (changed, mut changes) = tokio::sync::mpsc::unbounded_channel();
        let tracker = presence::Tracker::new(alice.clone(), Duration::from_secs(1), changed);
        tokio::time::timeout(Duration::from_secs(10), changes.recv())
            .await
            .unwrap();
        assert_eq!(tracker.supported(), Some(false));
        assert_eq!(tracker.get(alice.user_id().unwrap()), None);

        let syncing = sync::start(&alice, |_| {}).await.unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = rooms::watch_every(
            syncing.room_list_service(),
            Duration::from_secs(1),
            move |b| {
                let _ = tx.send(b);
            },
        );
        wait_for_buddies(&mut rx, "the seeded DMs in Buddies", |b| {
            has(b, "Bob Brown", rooms::Section::Buddies)
                && has(b, "Carol Chen", rooms::Section::Buddies)
        })
        .await;
        // Give the list's own check time to finish; nothing may move.
        tokio::time::sleep(Duration::from_secs(3)).await;
        let mut last = None;
        while let Ok(b) = rx.try_recv() {
            last = Some(b);
        }
        if let Some(b) = last {
            assert!(b.iter().all(|b| b.presence.is_none()), "{b:#?}");
            assert!(has(&b, "Bob Brown", rooms::Section::Buddies), "{b:#?}");
        }

        watcher.abort();
        syncing.stop().await;
        alice.logout().await.unwrap();
    }
}
