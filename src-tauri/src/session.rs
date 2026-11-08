//! Glue between the Matrix session and the app: starts and stops syncing,
//! broadcasts connection status and the Buddy List to windows, and handles a
//! revoked token.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use matrix_sdk::{
    Client,
    ruma::{OwnedRoomId, RoomId},
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    matrix::{
        self, Session, Signed, auto_reply,
        crypto::{self, Recovery},
        members::{self, ChatRoom},
        presence::{self, Own, Tracker},
        rooms::{self, Buddy},
        sync::{self, Status},
        timeline::{self, Line},
        upload,
        verify::{self, Flow, Step, Trust},
    },
    preferences, tray, windows,
};

/// Event carrying the new [`Status`] to every window.
pub const STATUS_EVENT: &str = "sync:status";
/// Event carrying the whole Buddy List (`Vec<Buddy>`) to every window.
pub const BUDDY_LIST_EVENT: &str = "buddylist:update";
/// Event carrying the new [`Recovery`] state to every window.
pub const RECOVERY_EVENT: &str = "recovery:status";
/// Event carrying whether this device is verified ([`Trust`]) to every window.
pub const TRUST_EVENT: &str = "verification:trust";
/// Event carrying our own presence ([`Own`]) to every window.
pub const PRESENCE_EVENT: &str = "presence:update";
/// Event carrying the verification's new [`Step`] to the Verify window.
pub const STEP_EVENT: &str = "verification:step";

/// How a session to [`begin`] came about.
pub enum Start {
    /// Resumed from the saved session at launch.
    Resumed(matrix::Account),
    /// Just signed on; saved by [`begin`].
    New(matrix::NewSession),
}

/// Make a session the app's session and start syncing it. A new sign-on is
/// saved first. There's only ever one session: if there already is one (a
/// slow sign-on finishing after another), the new one is ended instead.
pub async fn begin(app: &AppHandle, start: Start) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let state = app.state::<Session>();
    // Held throughout, so two sign-ons can't both get in.
    let mut signed = state.signed.lock().await;
    if signed.is_some() {
        if let Start::New(new) = start {
            new.discard().await;
        }
        return Err("You're already signed on.".to_owned());
    }
    let (account, new) = match start {
        Start::Resumed(account) => (account, false),
        Start::New(new) => {
            if let Err(e) = new.save(&data_dir).await {
                new.discard().await;
                return Err(e);
            }
            (new.account, true)
        }
    };
    match start_watching(app, account.client.clone()).await {
        Ok(watching) => {
            let user = account.client.user_id().map(ToString::to_string);
            tray::update(app, |tray| tray.user = user);
            *signed = Some(watching(account));
            Ok(())
        }
        Err(e) => {
            // A sign-on that can't sync isn't kept; a resumed session is,
            // to try again next launch.
            if new && let Err(e) = matrix::sign_off(account).await {
                eprintln!("{e}");
            }
            Err(e)
        }
    }
}

/// Start syncing `client` and everything that follows it. Returns how to
/// make the [`Signed`] once the caller has the account.
async fn start_watching(
    app: &AppHandle,
    client: Client,
) -> Result<impl FnOnce(matrix::Account) -> Signed, String> {
    let sync = sync::start(&client, {
        let app = app.clone();
        move |status| on_status(&app, status)
    })
    .await?;
    let presence = Tracker::new(client.clone(), presence::INTERVAL);
    let buddy_list = rooms::watch(sync.room_list_service(), presence.clone(), {
        let app = app.clone();
        move |buddies| on_buddies(&app, buddies)
    });
    let presence_events = {
        let app = app.clone();
        let presence = presence.clone();
        let mut changes = presence.subscribe();
        tauri::async_runtime::spawn(async move {
            while changes.changed().await.is_ok() {
                on_presence(&app, presence.own());
            }
        })
    };
    // Ask for the recovery key once per sign-on, not on every change.
    let asked = Arc::new(AtomicBool::new(false));
    let recovery = crypto::watch(&client, {
        let app = app.clone();
        move |state| on_recovery(&app, state, &asked)
    });
    let trust = verify::watch_trust(&client, {
        let app = app.clone();
        move |trust| on_trust(&app, trust)
    });
    let verification_requests = verify::on_requests(&client, {
        let app = app.clone();
        move |request| {
            let id = new_verification(&app);
            let flow = Flow::incoming(request, {
                let app = app.clone();
                move |step| on_step(&app, id, step)
            });
            start_verification(&app, flow);
        }
    });
    let auto_reply = auto_reply::watch(&client, presence.clone(), {
        let app = app.clone();
        move || app.state::<preferences::Store>().get().auto_reply
    });
    Ok(move |account: matrix::Account| Signed {
        client: account.client,
        store: account.store,
        sync,
        buddy_list,
        presence,
        presence_events,
        recovery,
        trust,
        verification_requests,
        auto_reply,
    })
}

/// Stop syncing and hand back the account, if signed on.
pub async fn end(app: &AppHandle) -> Option<matrix::Account> {
    let signed = app.state::<Session>().signed.lock().await.take()?;
    signed.buddy_list.abort();
    signed.presence_events.abort();
    signed.recovery.abort();
    signed.trust.abort();
    signed
        .client
        .remove_event_handler(signed.verification_requests);
    signed.auto_reply.stop(&signed.client);
    signed.sync.stop().await;
    let state = app.state::<Session>();
    *state.recovery.lock().unwrap() = Recovery::Unknown;
    *state.trust.lock().unwrap() = Trust::Unknown;
    *state.verification.lock().unwrap() = None;
    *state.verification_id.lock().unwrap() = 0;
    *state.verification_step.lock().unwrap() = None;
    app.state::<Session>().buddies.lock().unwrap().clear();
    tray::update(app, |tray| *tray = tray::State::default());
    for (_, upload) in app.state::<Session>().uploads.lock().unwrap().drain() {
        upload.abort();
    }
    // Dropping each watch stops it.
    app.state::<Session>().timelines.lock().unwrap().clear();
    app.state::<Session>().members.lock().unwrap().clear();
    for (label, window) in app.webview_windows() {
        if label.starts_with(windows::ROOM_LABEL_PREFIX)
            || label == windows::AWAY_LABEL
            || label == windows::ADD_BUDDY_LABEL
            || label == windows::RECOVERY_LABEL
            || label == windows::VERIFY_LABEL
            || label == windows::SETUP_RECOVERY_LABEL
        {
            let _ = window.close();
        }
    }
    Some(matrix::Account {
        client: signed.client,
        store: signed.store,
    })
}

/// A room's conversation as its window first gets it.
#[derive(Serialize)]
pub struct Conversation {
    /// Which watch this is. Its diffs carry the same number, so a window
    /// can tell them from those of a watch it replaced (after a reload).
    watch: u64,
    lines: Vec<Line>,
}

/// One change to a window's lines, from watch `watch`.
#[derive(Serialize)]
struct WatchedDiff {
    watch: u64,
    #[serde(flatten)]
    diff: timeline::Diff,
}

/// Numbers each timeline watch.
static WATCHES: AtomicU64 = AtomicU64::new(1);

/// Follow a room's conversation for its window, replacing any earlier
/// watch (e.g. after a reload). Returns the current lines; changes go to
/// the window as `room:<key>:timeline_diff` events, and the names of who's
/// typing as `room:<key>:typing`.
pub async fn watch_timeline(
    app: &AppHandle,
    client: &Client,
    room_id: &str,
) -> Result<Conversation, String> {
    let key = windows::room_key(room_id);
    let event = format!("room:{key}:timeline_diff");
    let typing_event = format!("room:{key}:typing");
    let label = format!("{}{key}", windows::ROOM_LABEL_PREFIX);
    let watch = WATCHES.fetch_add(1, Ordering::Relaxed);
    // An earlier watch (the window reloaded) stops first, so its diffs
    // don't mix with this one's.
    if let Some(old) = app
        .state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .remove(room_id)
    {
        old.stop();
    }
    let (lines, watching) = timeline::watch(
        client,
        room_id,
        {
            let (app, label) = (app.clone(), label.clone());
            move |diff| {
                let diff = WatchedDiff { watch, diff };
                if let Err(e) = app.emit_to(&label, &event, &diff) {
                    eprintln!("Couldn't send a timeline change to its window: {e}");
                }
            }
        },
        {
            let (app, label) = (app.clone(), label.clone());
            move |names| {
                if let Err(e) = app.emit_to(&label, &typing_event, &names) {
                    eprintln!("Couldn't send who's typing to its window: {e}");
                }
            }
        },
    )
    .await?;
    let replaced = app
        .state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .insert(room_id.to_owned(), Arc::new(watching));
    if let Some(old) = replaced {
        old.stop();
    }
    // The window may have closed while this was starting: then nothing will
    // remove the watch, so don't keep it.
    if app.get_webview_window(&label).is_none() {
        unwatch_timeline(app, room_id);
        return Err("That conversation isn't open.".to_owned());
    }
    subscribe_open_rooms(app).await;
    Ok(Conversation { watch, lines })
}

/// Follow who's in a room and its topic for its Chat Room window,
/// replacing any earlier watch. Returns them as they are now; changes go to
/// the window as the whole member list in `room:<key>:members` events, and
/// the new topic (or null) in `room:<key>:topic`.
pub async fn watch_chat_room(
    app: &AppHandle,
    client: &Client,
    room_id: &str,
) -> Result<ChatRoom, String> {
    let key = windows::room_key(room_id);
    let members_event = format!("room:{key}:members");
    let topic_event = format!("room:{key}:topic");
    let label = format!("{}{key}", windows::ROOM_LABEL_PREFIX);
    let (first, watching) = members::watch(
        client,
        room_id,
        {
            let (app, label) = (app.clone(), label.clone());
            move |members| {
                if let Err(e) = app.emit_to(&label, &members_event, &members) {
                    eprintln!("Couldn't send who's in the room to its window: {e}");
                }
            }
        },
        {
            let app = app.clone();
            move |topic| {
                if let Err(e) = app.emit_to(&label, &topic_event, &topic) {
                    eprintln!("Couldn't send the topic to its window: {e}");
                }
            }
        },
    )
    .await?;
    app.state::<Session>()
        .members
        .lock()
        .unwrap()
        .insert(room_id.to_owned(), watching);
    // As for the timeline: a window that closed meanwhile won't remove it.
    let label = format!("{}{key}", windows::ROOM_LABEL_PREFIX);
    if app.get_webview_window(&label).is_none() {
        app.state::<Session>()
            .members
            .lock()
            .unwrap()
            .remove(room_id);
        return Err("That conversation isn't open.".to_owned());
    }
    Ok(first)
}

/// Stop following a room's conversation and members, once its window is
/// gone.
pub fn unwatch_timeline(app: &AppHandle, room_id: &str) {
    if let Some(upload) = app
        .state::<Session>()
        .uploads
        .lock()
        .unwrap()
        .remove(room_id)
    {
        upload.abort();
    }
    let watching = app
        .state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .remove(room_id);
    app.state::<Session>()
        .members
        .lock()
        .unwrap()
        .remove(room_id);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Some(watching) = watching {
            watching.stop();
            // Closing mid-sentence: don't leave others seeing "typing…"
            // until the server times it out.
            if let Err(e) = watching.set_typing(false).await {
                eprintln!("Couldn't clear the typing notice: {e}");
            }
        }
        subscribe_open_rooms(&app).await;
    });
}

/// Have sync send rooms with an open window in full. The room list alone
/// gets one event per room per sync, so a burst of messages leaves a gap
/// and the open conversation jumps back to just the newest few.
async fn subscribe_open_rooms(app: &AppHandle) {
    // One at a time, each reading the open rooms once it runs: the server
    // replaces the whole set, so an older set applied last would drop a
    // room.
    let state = app.state::<Session>();
    let _one_at_a_time = state.subscribing.lock().await;
    let room_ids: Vec<OwnedRoomId> = app
        .state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .keys()
        .filter_map(|id| RoomId::parse(id).ok())
        .collect();
    let service = match app.state::<Session>().signed.lock().await.as_ref() {
        Some(signed) => signed.sync.room_list_service(),
        None => return,
    };
    let room_ids: Vec<&RoomId> = room_ids.iter().map(|id| &**id).collect();
    service.set_room_subscriptions(&room_ids).await;
}

/// Send a message to a room whose window is open: plain text, plus
/// optional formatted text.
pub async fn send_message(
    app: &AppHandle,
    room_id: &str,
    body: &str,
    html: Option<&str>,
) -> Result<(), String> {
    watching(app, room_id)?.send(body, html).await
}

/// Tell a room whose window is open whether we're typing.
pub async fn set_typing(app: &AppHandle, room_id: &str, typing: bool) -> Result<(), String> {
    watching(app, room_id)?.set_typing(typing).await
}

/// Mark a room whose window is open read.
pub async fn mark_read(app: &AppHandle, room_id: &str) -> Result<(), String> {
    watching(app, room_id)?.mark_read().await
}

/// An image's thumbnail, in a room whose window is open.
pub async fn thumbnail(app: &AppHandle, room_id: &str, event_id: &str) -> Result<String, String> {
    watching(app, room_id)?.thumbnail(event_id).await
}

/// The suggested file name for an attachment, in a room whose window is
/// open.
pub async fn attachment_name(
    app: &AppHandle,
    room_id: &str,
    event_id: &str,
) -> Result<String, String> {
    watching(app, room_id)?.attachment_name(event_id).await
}

/// Save an attachment to `path`, in a room whose window is open.
pub async fn save_attachment(
    app: &AppHandle,
    room_id: &str,
    event_id: &str,
    path: std::path::PathBuf,
) -> Result<(), String> {
    watching(app, room_id)?
        .save_attachment(event_id, path)
        .await
}

/// Load older history in a room whose window is open. Returns whether the
/// start of the room was reached.
pub async fn load_older(app: &AppHandle, room_id: &str) -> Result<bool, String> {
    watching(app, room_id)?.load_older().await
}

/// How sending files from a room window is going, for its status bar.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Upload {
    /// Sending file `index` (from 1) of `count`; `sent` of `total` bytes are
    /// up.
    #[serde(rename_all = "camelCase")]
    Sending {
        name: String,
        index: usize,
        count: usize,
        sent: usize,
        total: usize,
    },
    Done,
    Failed {
        message: String,
    },
    Cancelled,
}

/// Send files from a room's window, one after another, telling the window
/// how it goes as `room:<key>:upload` events. Checks them all against the
/// server's size limit first, so nothing is sent if one is too big.
pub async fn send_files(
    app: &AppHandle,
    room_id: &str,
    sources: Vec<upload::Source>,
) -> Result<(), String> {
    let watching = watching(app, room_id)?;
    if sources.is_empty() {
        return Ok(());
    }
    if busy_uploading(app, room_id) {
        return Err("Wait for the file being sent to finish, or cancel it.".to_owned());
    }
    // A server that doesn't say has no limit we can check.
    let max = watching.max_upload_size().await;
    // Asking took a moment: if the window closed meanwhile, nothing would
    // show the upload or be able to cancel it.
    let key = windows::room_key(room_id);
    let label = format!("{}{key}", windows::ROOM_LABEL_PREFIX);
    if app.get_webview_window(&label).is_none() {
        return Err("That conversation isn't open.".to_owned());
    }
    for source in &sources {
        let (name, size) = (source.name()?, source.size()?);
        if let Some(max) = max
            && size > max
        {
            return Err(format!(
                "{name} is too big to send ({}). This server takes files up to {}.",
                upload::describe_size(size),
                upload::describe_size(max)
            ));
        }
    }

    let event = format!("room:{key}:upload");
    let emit = {
        let app = app.clone();
        move |upload: Upload| {
            if let Err(e) = app.emit_to(&label, &event, &upload) {
                eprintln!("Couldn't send upload progress to its window: {e}");
            }
        }
    };
    let state = app.state::<Session>();
    let mut uploads = state.uploads.lock().unwrap();
    // Checked again here, in case another send started meanwhile.
    if uploads
        .get(room_id)
        .is_some_and(|task| !task.inner().is_finished())
    {
        return Err("Wait for the file being sent to finish, or cancel it.".to_owned());
    }
    let count = sources.len();
    let task = tauri::async_runtime::spawn(async move {
        for (i, source) in sources.into_iter().enumerate() {
            let name = source.name().unwrap_or_default();
            let sending = |sent, total| Upload::Sending {
                name: name.clone(),
                index: i + 1,
                count,
                sent,
                total,
            };
            emit(sending(0, 0));
            let prepared =
                match tauri::async_runtime::spawn_blocking(move || upload::prepare(source)).await {
                    Ok(Ok(prepared)) => prepared,
                    Ok(Err(message)) => return emit(Upload::Failed { message }),
                    Err(e) => {
                        return emit(Upload::Failed {
                            message: e.to_string(),
                        });
                    }
                };
            // Every whole percent is plenty for a status bar.
            let last = std::sync::atomic::AtomicUsize::new(usize::MAX);
            // The SDK sends a picture's thumbnail alongside it and reports
            // both through here; follow only the biggest, the file itself,
            // or the bar jumps between them.
            let biggest = std::sync::atomic::AtomicUsize::new(0);
            let result = watching
                .send_file(prepared, |sent, total| {
                    if total < biggest.fetch_max(total, Ordering::Relaxed) {
                        return;
                    }
                    let percent = sent * 100 / total.max(1);
                    if last.swap(percent, Ordering::Relaxed) != percent {
                        emit(sending(sent, total));
                    }
                })
                .await;
            if let Err(message) = result {
                return emit(Upload::Failed { message });
            }
        }
        emit(Upload::Done);
    });
    uploads.insert(room_id.to_owned(), task);
    Ok(())
}

fn busy_uploading(app: &AppHandle, room_id: &str) -> bool {
    app.state::<Session>()
        .uploads
        .lock()
        .unwrap()
        .get(room_id)
        .is_some_and(|task| !task.inner().is_finished())
}

/// Stop sending files from a room's window. A file already uploaded may
/// still arrive.
pub fn cancel_upload(app: &AppHandle, room_id: &str) {
    let Some(task) = app
        .state::<Session>()
        .uploads
        .lock()
        .unwrap()
        .remove(room_id)
    else {
        return;
    };
    if !task.inner().is_finished() {
        task.abort();
        let key = windows::room_key(room_id);
        let label = format!("{}{key}", windows::ROOM_LABEL_PREFIX);
        let _ = app.emit_to(&label, &format!("room:{key}:upload"), Upload::Cancelled);
    }
}

/// Whether a room's window is open and following its conversation.
pub fn is_open(app: &AppHandle, room_id: &str) -> bool {
    app.state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .contains_key(room_id)
}

/// The watch of an open room window's conversation. Cloned out so the map
/// isn't locked while it's used.
fn watching(app: &AppHandle, room_id: &str) -> Result<Arc<timeline::Watching>, String> {
    app.state::<Session>()
        .timelines
        .lock()
        .unwrap()
        .get(room_id)
        .cloned()
        .ok_or_else(|| "That conversation isn't open.".to_owned())
}

fn on_status(app: &AppHandle, status: Status) {
    eprintln!("Sync status: {status:?}");
    *app.state::<Session>().status.lock().unwrap() = status.clone();
    if let Err(e) = app.emit(STATUS_EVENT, &status) {
        eprintln!("Couldn't send sync status to windows: {e}");
    }
    if status == Status::SignedOut {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { signed_out(&app).await });
    }
}

fn on_presence(app: &AppHandle, own: Own) {
    let away = own.supported.filter(|&s| s).map(|_| own.away.is_some());
    tray::update(app, |tray| tray.away = away);
    if let Err(e) = app.emit(PRESENCE_EVENT, &own) {
        eprintln!("Couldn't send your presence to windows: {e}");
    }
}

fn on_buddies(app: &AppHandle, buddies: Vec<Buddy>) {
    let unread = buddies.iter().filter(|b| b.unread).count();
    tray::update(app, |tray| tray.unread = unread);
    *app.state::<Session>().buddies.lock().unwrap() = buddies.clone();
    if let Err(e) = app.emit(BUDDY_LIST_EVENT, &buddies) {
        eprintln!("Couldn't send the Buddy List to windows: {e}");
    }
}

/// The server rejected our token (e.g. signed out from another client): forget
/// the session and go back to Sign On, explaining why.
async fn signed_out(app: &AppHandle) {
    if let Some(account) = end(app).await
        && let Err(e) = matrix::forget(account).await
    {
        eprintln!("{e}");
    }
    if let Err(e) = windows::open_sign_on(app, windows::SignOnNotice::SignedOut) {
        eprintln!("Couldn't open Sign On: {e}");
    }
    windows::close_buddy_list(app);
}

/// Sign off (from the Buddy List or the tray): end the session on the
/// server, forget it here, and go back to Sign On. Always ends up at Sign
/// On: a sign-off that half fails is still a sign-off.
pub async fn sign_off(app: &AppHandle) {
    let forgotten = match end(app).await {
        Some(account) => matrix::sign_off(account).await,
        // Nothing running (resuming failed): forget what's saved.
        None => match app.path().app_data_dir() {
            Ok(data_dir) => matrix::forget_saved(&data_dir).await,
            Err(e) => Err(e.to_string()),
        },
    };
    if let Err(e) = forgotten {
        eprintln!("Couldn't forget the session: {e}");
    }
    if let Err(e) = windows::open_sign_on(app, windows::SignOnNotice::None) {
        eprintln!("Couldn't open Sign On: {e}");
    }
    windows::close_buddy_list(app);
}

/// Go away with `message`, or come back with `None`.
pub async fn set_away(app: &AppHandle, message: Option<&str>) -> Result<(), String> {
    let presence = app
        .state::<Session>()
        .signed
        .lock()
        .await
        .as_ref()
        .map(|s| s.presence.clone())
        .ok_or("Not signed on.")?;
    presence.set_away(message).await
}

/// Share the new recovery state, and open Restore Messages the first time
/// this device turns out to need the recovery key.
fn on_recovery(app: &AppHandle, state: Recovery, asked: &AtomicBool) {
    *app.state::<Session>().recovery.lock().unwrap() = state;
    if let Err(e) = app.emit(RECOVERY_EVENT, state) {
        eprintln!("Couldn't send the recovery state to windows: {e}");
    }
    if state == Recovery::Incomplete
        && !asked.swap(true, Ordering::Relaxed)
        && let Err(e) = windows::open_recovery(app)
    {
        eprintln!("Couldn't open Restore Messages: {e}");
    }
}

/// Share whether this device is verified.
fn on_trust(app: &AppHandle, trust: Trust) {
    *app.state::<Session>().trust.lock().unwrap() = trust;
    if let Err(e) = app.emit(TRUST_EVENT, trust) {
        eprintln!("Couldn't send whether this device is verified: {e}");
    }
}

/// Numbers each verification.
static VERIFICATIONS: AtomicU64 = AtomicU64::new(1);

/// Start numbering a new verification, which becomes the current one, and
/// return its number for [`on_step`]. Call before creating its [`Flow`],
/// which may report its first step straight away.
pub fn new_verification(app: &AppHandle) -> u64 {
    let id = VERIFICATIONS.fetch_add(1, Ordering::Relaxed);
    let state = app.state::<Session>();
    *state.verification_id.lock().unwrap() = id;
    *state.verification_step.lock().unwrap() = None;
    id
}

/// Share verification `id`'s new step with the Verify window. Steps from
/// any verification but the current one (an earlier one still winding
/// down, or one from before signing off) are dropped.
pub fn on_step(app: &AppHandle, id: u64, step: Step) {
    let state = app.state::<Session>();
    if *state.verification_id.lock().unwrap() != id {
        return;
    }
    *state.verification_step.lock().unwrap() = Some(step.clone());
    if let Err(e) = app.emit_to(windows::VERIFY_LABEL, STEP_EVENT, &step) {
        eprintln!("Couldn't send the verification step: {e}");
    }
}

/// Make `flow` the verification in progress, replacing any earlier one,
/// and show it in the Verify window.
pub fn start_verification(app: &AppHandle, flow: Flow) {
    let state = app.state::<Session>();
    // The flow may already have reported its first step.
    *state.verification.lock().unwrap() = Some(Arc::new(flow));
    if let Err(e) = windows::open_verify(app) {
        eprintln!("Couldn't open Verify: {e}");
    }
}

/// Forget the verification (its window closed), cancelling it on both sides
/// if it's still going.
pub fn end_verification(app: &AppHandle) {
    let state = app.state::<Session>();
    let flow = state.verification.lock().unwrap().take();
    let step = state.verification_step.lock().unwrap().take();
    let finished = matches!(step, Some(Step::Done { .. } | Step::Cancelled { .. }));
    if let Some(flow) = flow
        && !finished
    {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = flow.cancel().await {
                eprintln!("{e}");
            }
        });
    }
}
