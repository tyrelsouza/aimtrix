# Backend: the Rust core

Everything in `src-tauri/`. It's a Rust 2024 crate named `aimtrix` that builds a library, `aimtrix_lib`, and a small binary that calls it (Tauri's usual layout, needed for mobile targets and to avoid a Windows name clash).

```
src-tauri/
  Cargo.toml            dependencies; release profile (LTO, strip); dev builds optimize the hash and image crates
  tauri.conf.json       app identity, bundle settings (.deb/.AppImage/.dmg), resources; no windows here
  capabilities/default.json   every window label and the permissions windows get
  build.rs              tauri_build::build()
  about.toml            licenses cargo-about accepts (third-party notices)
  aimtrix.desktop       Linux desktop entry template
  icons/                app icons (original art)
  src/
    main.rs             process entry
    lib.rs              run(): builder, state, commands; startup()
    commands.rs         the #[tauri::command] surface
    session.rs          the signed-on session's lifecycle; events to windows
    windows.rs          creating windows
    tray.rs             tray icon (Windows, Linux) and Dock badge (macOS)
    preferences.rs      preferences.json
    matrix/
      mod.rs            Session state; sign on, resume, sign off; server checks; most server tests
      keyring.rs        the saved session in the OS keyring
      sync.rs           SyncService, connection status, token refresh, member fixes
      rooms.rs          room list → Buddy List: DM vs group, groups, Add Buddy, block, invites
      timeline.rs       a room's Timeline → Lines and Diffs; send, files, typing, read
      members.rs        Chat Room member list and topic
      presence.rs       presence heartbeat, buddy polling, away messages
      auto_reply.rs     answering IMs while away
      crypto.rs         encryption settings, recovery (restore and set up)
      verify.rs         SAS emoji verification and device trust
      upload.rs         preparing files: type, size, image thumbnails; clipboard
      media.rs          Buddy Icons as data: URLs, cached
      html.rs           sanitizing message HTML
```

## Entry points

### `main.rs`

`main()` does one thing before handing over: on Linux it sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` (unless set already), because WebKitGTK's DMA-BUF renderer often fails to redraw a covered window. Then it calls `aimtrix_lib::run()`. The `windows_subsystem = "windows"` attribute stops a console window appearing on Windows release builds.

### `lib.rs`: `run()`

Builds the Tauri app:

| Step | What |
|---|---|
| Plugins | `tauri_plugin_dialog` (Open/Save dialogs, confirmations), `tauri_plugin_opener` (open the browser for OAuth, open license files). |
| Managed state | `matrix::Session` (the session and the latest values windows ask for), `commands::BrowserSignOn` (cancelling a browser sign-on), `tray::Tray` (what the tray shows). |
| `setup` | Opens `preferences::Store` (needs the app handle for its path), creates the tray icon if the system has one, and spawns `startup()`. |
| `invoke_handler` | `generate_handler![…]` with every command. A command not listed here doesn't exist to the frontend. |
| Run loop | On macOS, an exit request with no code (the last window closed) is prevented, and a Dock click with no visible windows calls `tray::open`. |

`#![recursion_limit = "256"]` at the top is needed because matrix-sdk futures nest deeply (the room window task in `timeline.rs` passes the default limit).

### `lib.rs`: `startup()`

Resumes the saved session straight into the Buddy List, or opens Sign On. `matrix::resume` works offline (the token is only checked once sync starts). If resuming fails, for example because the store is damaged, the saved session is forgotten and Sign On opens. If no window can be opened at all, the app exits, since there's nowhere to show an error.

## App-wide state: `matrix::Session`

Defined in `matrix/mod.rs`, managed by Tauri, and reachable from any command as `State<'_, Session>` or from anywhere with `app.state::<Session>()`.

| Field | Type | Purpose |
|---|---|---|
| `signed` | async `Mutex<Option<Signed>>` | The signed-on client and every running watcher. `None` when signed off. |
| `status` | `Mutex<sync::Status>` | Latest connection status, for windows that open late. |
| `buddies` | `Mutex<Vec<Buddy>>` | Latest Buddy List. Also how `open_room` finds a room's name and kind. |
| `timelines` | `Mutex<HashMap<room ID, Arc<timeline::Watching>>>` | The conversation each open room window follows. Dropping one stops it. |
| `uploads` | `Mutex<HashMap<room ID, JoinHandle>>` | The file-sending task per room window. |
| `members` | `Mutex<HashMap<room ID, members::Watching>>` | Member and topic watches per open Chat Room. |
| `recovery`, `trust` | `Mutex<…>` | Latest recovery state and device trust. |
| `verification`, `verification_step` | `Mutex<Option<…>>` | The verification in progress and its latest step. |

`Signed` holds the `Client`, `sync::Syncing`, the Buddy List task, the presence `Tracker` and its event task, the recovery and trust tasks, the verification request handler and the auto-reply handlers.

Two kinds of mutex are used on purpose. `signed` is an async mutex because it's held across awaits; the rest are `std::sync::Mutex`, held only briefly and never across an await (a `std` guard isn't `Send`). The habit throughout is to clone what's needed out of the lock, release it, then do the work: `commands::client()` clones the `Client`, and `session::watching()` clones the `Arc<Watching>`.

## `commands.rs`: the command surface

Every `#[tauri::command]` is here, and each is thin: get state, call into `session`, `matrix` or `windows`, and turn errors into a `String` for the window. The [IPC reference](ipc.md) lists them all.

Conventions:
- **Errors are `Result<T, String>`**, worded for the user ("That conversation isn't open."). Windows show them as they are.
- Arguments are snake_case in Rust and camelCase from JS (`room_id` ↔ `roomId`); Tauri converts them.
- Commands that need the calling window take `window: WebviewWindow`: `login` closes Sign On, and dialogs are parented to the window that asked.
- Confirmations happen in Rust with native dialogs: blocking a buddy, sending a pasted picture.
- `send_pasted` takes a raw `tauri::ipc::Request`: the picture is the request body, and the room ID and MIME type are the `x-room-id` and `x-mime-type` headers. That's much faster than JSON for big images.
- `BrowserSignOn` holds the cancel sender for a browser sign-on in progress. Starting another replaces it, which cancels the first.

## `session.rs`: the session's lifecycle

The glue between the Matrix layer and the app. It owns the event names for app-wide events:

| Constant | Event | Payload |
|---|---|---|
| `STATUS_EVENT` | `sync:status` | `sync::Status` |
| `BUDDY_LIST_EVENT` | `buddylist:update` | `Vec<rooms::Buddy>` |
| `RECOVERY_EVENT` | `recovery:status` | `crypto::Recovery` |
| `TRUST_EVENT` | `verification:trust` | `verify::Trust` |
| `PRESENCE_EVENT` | `presence:update` | `presence::Own` |
| `STEP_EVENT` | `verification:step` (to the Verify window only) | `verify::Step` |

Main functions:
- **`begin(app, client)`** starts sync and every watcher and stores them in `Session.signed` (see [Architecture: sign on](architecture.md#sign-on) for the order). Each watcher gets a callback (`on_status`, `on_buddies`, `on_presence`, `on_recovery`, `on_trust`) that stores the latest value, emits the event and updates the tray.
- **`end(app)`** reverses it: aborts tasks, removes handlers, stops sync, resets the stored values and the tray, aborts uploads, drops timeline and member watches, and closes room windows and session dialogs. It returns the `Client` so the caller can sign off with it.
- **`sign_off(app)`** (Sign Off button or tray): `end`, then `matrix::sign_off` (or `forget` if not signed on), open Sign On, destroy the Buddy List.
- **`signed_out(app)`** runs when sync reports `Status::SignedOut` (the server rejected the token): `end`, `forget`, Sign On with `?signed-out`.
- **`watch_timeline` / `watch_chat_room` / `unwatch_timeline`** start and stop a room window's watches. They build the room-scoped event names (`room:<key>:timeline_diff`, `:typing`, `:members`, `:topic`) and emit them only to `room-<key>`. After a change they call `subscribe_open_rooms`, which asks sliding sync to send open rooms in full. Otherwise a burst of messages leaves a gap and the conversation jumps back to the newest few.
- **Room actions** (`send_message`, `set_typing`, `mark_read`, `thumbnail`, `attachment_name`, `save_attachment`, `load_older`) all go through `watching(app, room_id)`, which fails with "That conversation isn't open." if the room has no window.
- **`send_files`** checks every file against the server's size limit first, then sends them one by one in a task, emitting `Upload` progress (`sending` with byte counts, at most once per whole percent, then `done`, `failed` or `cancelled`) as `room:<key>:upload`. One upload per room at a time; `cancel_upload` aborts it.
- **Verification:** `start_verification` stores a `Flow` and opens Verify; `on_step` stores and emits each step; `end_verification` (Verify window destroyed) cancels a flow that hasn't finished.
- **`set_away`** goes through the presence `Tracker`.

## `windows.rs`: creating windows

Every window comes from `aim_window(app, label, url)`, a `WebviewWindowBuilder` with `decorations(false)`, since the frontend draws the title bar and frame. Sizes are content sizes plus `FRAME_WIDTH` (6) and `FRAME_HEIGHT` (24).

| Function | Label | Route | Notes |
|---|---|---|---|
| `open_buddy_list` | `main` | `/` | Resizable. If it exists: show, unminimize, focus (it may be hidden in the tray). With the tray shown and "close to tray" on, closing hides it instead. |
| `close_buddy_list` | `main` | | Destroys it, bypassing close to tray (used when signing off). |
| `open_sign_on(signed_out)` | `signon` | `/signon` or `/signon?signed-out` | Fixed size, centred. |
| `open_room(room_id, name, kind)` | `room-<key>` | `/im?room=…` (DM) or `/chat?room=…` (group) | Title is the name (" - Chat Room" added for groups). On `Destroyed` it calls `session::unwatch_timeline`. |
| `open_recovery` | `recovery` | `/recovery` | Restore Messages. |
| `open_setup_recovery` | `setup-recovery` | `/setup-recovery` | |
| `open_verify` | `verify` | `/verify` | On `Destroyed` it calls `session::end_verification`. |
| `open_away` | `away` | `/away` | |
| `open_add_buddy` | `add-buddy` | `/add-buddy` | |
| `open_preferences` | `preferences` | `/preferences` | |
| `open_about` | `about` | `/about` | |

Each `open_*` focuses (and unminimizes) the window if it's already open. `room_key` is FNV-1a of the room ID as 16 hex digits, chosen because it's stable across runs and Rust versions, unlike `DefaultHasher`. Room IDs go in the URL percent-encoded (`encode_query`).

## `tray.rs`

The tray icon on Windows and Linux; on macOS the same state drives the Dock badge.
- **`available()`**: on Linux, tries to load an AppIndicator library with `libloading`, because the tray crate panics without one. Always true on Windows; false elsewhere.
- **`create(app)`** builds the icon, tooltip and menu from `State { user, away, unread }`. A left click opens the Buddy List (or Sign On).
- **`update(app, change)`** applies a change to the state and redraws only if something changed. `session.rs` calls it on sign-on, sign-off, presence and Buddy List changes.
- **Menu:** Open Buddy List / Sign On…, Away… / I'm Back (disabled without presence), Sign Off, Quit AIMtrix. The actions call `windows::open_away`, `session::set_away(None)`, `session::sign_off` and `app.exit(0)`.
- **Icon:** drawn from `icons/64x64.png`: greyed and faded when signed off, a slate dot when away, a red dot for unread. Unit-tested pixel by pixel.

## `preferences.rs`

`Preferences { auto_reply, text_color, font_face, font_size, close_to_tray }`, serialized camelCase, with `#[serde(default)]` so older files missing newer fields still load. `checked()` puts out-of-range values back to their defaults: colors must be `#rrggbb`, and fonts must be in `FONT_FACES`, since they end up in CSS. `Store::set` saves atomically and emits `preferences:update` to every window. `close_to_tray` defaults to on for Windows and off elsewhere.

## The Matrix layer: `matrix/`

### `mod.rs`: sign on, resume, sign off

- **`sign_on(data_dir, homeserver, user, password)`** and **`sign_on_with_browser(data_dir, homeserver, open, cancelled)`** both go through `new_sign_on`. It creates a fresh store with a random passphrase, runs the login, saves the session to the keyring, and deletes the store if anything fails.
- **`login`** discovers the server (a server name via `.well-known`, or a URL), calls `check_sliding_sync` (refusing servers without MSC4186 before any `/login`, so no device is created), then logs in. Wrong-password errors are reworded; other refusals show the server's own message (`forbidden`).
- **`browser_login`** uses matrix-sdk's OAuth: dynamic client registration each time (the loopback redirect port changes), authorization code with PKCE, and the SDK's `local-server` to catch the redirect. The redirect URI is registered without its port (RFC 8252), which continuwuity requires. It gives up after `BROWSER_TIMEOUT` (10 minutes) or when cancelled. `client_metadata` describes AIMtrix to the server (`CLIENT_URI` is the GitHub repo).
- **`sign_on_methods(homeserver)`** uses a throwaway client to read the login flows and OAuth metadata, returning `Methods { password, browser }`.
- **`resume(data_dir)`** loads the keyring entry and `restore`s the client from the store.
- **`save_refreshed_tokens`** rewrites the keyring entry when the SDK refreshes OAuth tokens, but only if the saved session is still this one (`keyring::update` under its lock).
- **`sign_off(data_dir, client)`** calls `log_out` (deletes the device; for OAuth it revokes the token, falling back to `revoke_over_http` on plain-http dev servers, where the SDK refuses), then `forget`, which clears the keyring and the store. Local cleanup happens even if the server can't be reached.
- **`builder(store, passphrase)`** is the one place clients are configured: SQLite store, `crypto::settings()`, `handle_refresh_tokens()`.
- **`sliding_sync_gone`** asks the server again, ignoring the cached answer, when sync fails, so the status bar can say the server dropped sliding sync instead of showing a raw error.

Everything after `#[cfg(test)]` (most of the file) is tests; see [Development](development.md#tests).

### `keyring.rs`

`SavedSession { homeserver, store_passphrase, auth }`, where `SavedAuth` is untagged: `Password { session }` or `OAuth { client_id, oauth }`. Untagged so sessions saved before OAuth existed still load. Load errors deliberately drop the serde message, which could quote the token. A global `CHANGING` mutex serializes `save`, `update` and `forget`, so a token refresh during sign-off can't write the session back. The calls block, so async code runs them in `spawn_blocking`.

### `sync.rs`

`start(client, on_status)` builds matrix-sdk-ui's `SyncService` with offline mode, which reconnects by itself. A watcher task maps its states to `Status` (`connecting`, `online`, `offline`, `error`, `signed-out`). On an error it explains the cause if the server lost sliding sync, waits `RETRY_AFTER` (5 s), and restarts. It also listens to session changes: `UnknownToken` becomes `SignedOut`, and `TokensRefreshed` saves the new tokens.

`notice_member_changes` is an event handler for servers (continuwuity) that don't put timeline member events in sliding sync's required state. When a member event doesn't match the stored member, it calls `mark_members_missing` so the SDK reloads the members.

`Syncing::stop` repeats the stop request every 100 ms until it takes. That works around matrix-sdk 0.19 losing a stop request sent before the room list sync first polls.

### `rooms.rs`: the Buddy List model

The classification rules are pure functions over `RoomFacts` (membership, `m.direct`, member count, favourite tag, `aimtrix.chat` tag, the other person's presence), so they're unit-tested without a server:
- **`kind`**: a DM if listed in `m.direct` **or** it has at most two joined and invited members, unless tagged `aimtrix.chat` (`CHAT_TAG`), which forces a Chat Room.
- **`section`**: Invitations, Favorites, Chats for groups; for DMs, Buddies when the server has no presence, otherwise Online or Offline. Left rooms are dropped.

`watch(service, presence, on_update)` follows `RoomListService::all_rooms()` (non-left rooms, up to 10,000, most recent first) and rebuilds the whole `Vec<Buddy>` whenever the list changes, a Buddy Icon finishes downloading, a buddy's presence changes or the ignore list changes. Lists are small, so there's no diffing.

`Buddy` (camelCase over IPC): `roomId`, `name`, `kind`, `section`, `unread`, `icon`, `presence`, `awayMessage`, `userId`, `blocked`.

Also here: `add_buddy` (parse a Matrix ID or a bare name on our server, reuse an existing DM, or create an encrypted DM listed in `m.direct`; `existing_dm` remembers DMs made this run, since `m.direct` and members arrive only with a later sync), `set_blocked` (the ignore list), `accept` / `decline` invites, and `icon` for the IM window's larger Buddy Icon.

### `timeline.rs`: conversations

`watch(client, room_id, on_diff, on_typing)` opens the room's matrix-sdk-ui `Timeline` and returns the current lines plus a `Watching`. Its task:
- loads a first page of history (`PAGE` = 50) and the members together, so senders have display names;
- forwards every `VectorDiff` as a `Diff` (`append`, `clear`, `pushFront`, `pushBack`, `popFront`, `popBack`, `insert`, `set`, `remove`, `truncate`, `reset`);
- asks the SDK to load replied-to messages that aren't loaded, once each;
- reloads a page of history after the SDK clears the timeline (it does that on block and unblock);
- turns typing notifications into display names.

**`Line`** is tagged by `type`: `message` (text, HTML, emote, edited, media, reply, failed), `deleted`, `undecryptable` (with an `UndecryptableReason`: `waiting`, `before-sign-on`, `before-joined`, `withheld`, `untrusted`), `membership`, `topic`, `start` (no more history) and `other` (not shown yet). Every timeline item becomes exactly one line, including `other`, so indexes line up.

`Watching` methods: `send` (HTML sanitized, local echo through the timeline), `send_file`, `max_upload_size`, `load_older`, `set_typing`, `mark_read`, `thumbnail` (480×360 from the server, or the full image up to 10 MB for encrypted images, which the server can't shrink), `attachment_name` and `save_attachment` (decrypted if need be; sender-chosen names are reduced to their last part by `safe_file_name`).

### `members.rs`

`watch(client, room_id, on_members, on_topic)` returns the first `ChatRoom { members, topic }`. After that it sends the **whole** sorted member list on every member event or icon download, and the topic when it changes. `list` reloads members from the server when the stored count disagrees with the room's joined count (another continuwuity fix).

### `presence.rs`

Sliding sync carries no presence, and many servers turn it off, so `Tracker` does it by hand:
- **`detect`**: set ourselves online and read it back. A server without presence says "offline" (or refuses with 403); a 404 means try again.
- **Heartbeat** every 20 s keeps our presence and away message fresh.
- **Polling** every 20 s (`INTERVAL`) checks the buddies the Buddy List asked about, and new ones at once.
- `get(user)` returns `None` when the server has no presence; the Buddy List then falls back to recent activity.
- `set_away(message)` (up to `MAX_AWAY_MESSAGE` = 1024 characters) sets `unavailable` with a `status_msg`; `None` comes back online. `own()` is what `presence:update` carries: `{ supported, away }`.
- `sign_off` sets us offline before signing off.

### `auto_reply.rs`

While away and with the preference on, it answers each DM's first new message with `"Auto-reply: " + away message` as an `m.notice`, once per conversation per time away. Notices are never answered, so two people who are both away can't loop. The `Replied::decide` / `skip_reason` rules are unit-tested. Encrypted messages that arrive before their key are retried for up to 60 s.

### `crypto.rs`

- `settings()`: create a cross-signing identity at sign-on if the account has none, and download every room key from the backup once recovery is unlocked.
- `watch` reports `Recovery`: `unknown`, `enabled`, `disabled` or `incomplete` (this device needs the recovery key; `session` opens Restore Messages once).
- `recover(secret)` takes the recovery key or passphrase. `setup_check` returns `Setup` (`ready`, `verify-first`, `already-set-up`). `set_up` creates a backup and recovery and returns the key once. Errors never include the key.

### `verify.rs`

`Flow` follows one SAS (emoji) verification, whichever side asked, reporting `Step`s (tagged by `step`): `waiting`, `incoming`, `starting`, `compare` (seven `Emoji`), `confirmed`, `done`, `cancelled`. Methods: `request`, `incoming`, `accept`, `start`, `confirm(matched)`, `cancel`. `on_requests` only accepts requests from your own other sessions. `trust` / `watch_trust` report `Trust` (`unknown`, `no-cross-signing`, `verified`, `unverified`).

### `upload.rs`, `media.rs`, `html.rs`

- **`upload.rs`**: `Source` is a file path or pasted bytes. `prepare` (blocking) reads the file, guesses the MIME type and, for images, reads the dimensions and makes a thumbnail within 800×600 (skipped for sources over 30 MB). `clipboard_picture` reads the system clipboard with `arboard`, for pastes WebKitGTK doesn't hand over. `describe_size` formats sizes for people.
- **`media.rs`**: `Icons` keeps Buddy Icons as ready `data:` URLs in memory and starts each download on first request, signalling the Buddy List to rebuild when it lands. The SDK also caches the downloads in the store.
- **`html.rs`**: `sanitize` (with `ammonia`) keeps the tags the Matrix spec suggests and drops scripts, styles, event handlers, unknown attributes and links that aren't web, mail or Matrix links. Colors survive only as `#rrggbb`. It runs on both incoming and outgoing HTML, so windows can render it as it is.

## Conventions

- **Never log secrets.** No access tokens, refresh tokens, recovery keys, passphrases or message text in logs or error strings. Errors from serde or the SDK that might quote them are replaced.
- **Logging** is `eprintln!` for things the user can't act on (a window that couldn't be sent an event, a tray that couldn't redraw). Errors the user should see go back to the window as `Err(String)`.
- **Tests live next to the code** in `#[cfg(test)] mod tests`. Tests that need a homeserver are `#[ignore = "needs the dev homeserver"]`; see [Development](development.md#tests).
- **Comments say why.** Most functions have a `///` doc comment; workarounds for SDK or server behaviour explain what they work around.

## Recipes

### Add a command
1. Write `#[tauri::command] pub async fn my_thing(session: State<'_, Session>, room_id: String) -> Result<T, String>` in `commands.rs`, with a doc comment saying what it does and which event carries later changes, if any.
2. Add `commands::my_thing` to `generate_handler![…]` in `lib.rs`.
3. Make `T` `Serialize` with `#[serde(rename_all = "camelCase")]` (or `kebab-case` for enum values), and mirror it in a `src/lib/*.ts` type with a "Mirrors `T` in …" comment.
4. Add it to [ipc.md](ipc.md).

### Add an event
- App-wide: add a `*_EVENT` constant in `session.rs`, store the latest value in `Session` if late windows need it (plus a command to read it), and `app.emit(…)`.
- For one room window: build `room:{key}:<name>` with `windows::room_key` and `emit_to` the `room-<key>` label.
- In the frontend, add a `watch…` function that listens first, then asks (see [Frontend](frontend.md#the-watch-pattern)). Add it to [ipc.md](ipc.md).

### Add a window
1. Add an `open_*` function and a `*_LABEL` constant in `windows.rs`, built with `aim_window` and with the frame added to its size.
2. Add the label to `src-tauri/capabilities/default.json`. **Easy to forget:** without it, the window's commands work but it can't listen to events or close itself.
3. If it belongs to the session, close it in `session::end`.
4. Add a route in `src/routes/<name>/+page.svelte` and an `open_*` command if windows need to open it.
