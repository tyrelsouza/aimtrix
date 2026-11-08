# Architecture

## In one paragraph

AIMtrix is a [Tauri v2](https://v2.tauri.app) desktop app. One native process runs a Rust core that owns the **only** Matrix session: a single matrix-sdk `Client` with an encrypted SQLite store and end-to-end encryption. Every window (Sign On, the Buddy List, each IM and Chat Room, the small dialogs) is a separate webview showing one route of a SvelteKit single-page app. Windows hold no Matrix state and load no Matrix SDK. They call into Rust with **commands** (request/response) and Rust pushes changes to them with **events**.

```
┌──────────────────────────── one native process ─────────────────────────────┐
│                                                                             │
│   Rust core (src-tauri/src)                                                 │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │ lib.rs: run() and startup()                                          │   │
│   │ commands.rs: #[tauri::command]s ◄──── invoke("…") from any window    │   │
│   │ session.rs: lifecycle, broadcasts ────► app.emit / emit_to(label)    │   │
│   │ windows.rs, tray.rs, preferences.rs                                  │   │
│   │ matrix/: Client, sync, rooms, timeline, crypto, verify, presence…    │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│        │ HTTPS: sliding sync, media, keys        │ keyring    │ files      │
│        ▼                                         ▼            ▼            │
│   homeserver                              OS keyring    matrix-store/      │
│                                                         preferences.json   │
│                                                                             │
│   Webviews, one per window, each running the SvelteKit SPA at one route:    │
│   [signon] [main = Buddy List] [room-<key> = IM/Chat] [away] [verify] …     │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Why the SDK lives in Rust

Tauri gives each window its own JavaScript context. A JS Matrix client would have to live in one window and relay to the others, and its IndexedDB crypto store doesn't cope with being shared. In the Rust core, one `Client` serves every window, and the parts that matter most (E2EE, sliding sync, `RoomListService`, `Timeline`) are the same well-used code Element X runs on. The trade-off is that every feature a window needs has to be exposed as a command or event (see [IPC](ipc.md)).

## The rules this produces

- **One session.** `matrix::Session` (managed Tauri state) holds the signed-on client in `signed: Mutex<Option<Signed>>`. `None` means signed off. Everything that needs the client clones it out of there.
- **Windows are views.** A window asks for the current value with a command, listens for changes with an event, and renders. All decisions (is this a DM? which group? is this HTML safe?) are made in Rust, where they're unit-tested.
- **Windows are created in Rust**, in `windows.rs`, never in `tauri.conf.json` (its `windows` list is empty). Each label must also be in `src-tauri/capabilities/default.json`, or the window can't listen to events or close itself. Commands still work without it, which makes the mistake easy to miss.
- **One window per room.** Room windows are labeled `room-<key>`, where the key is a stable hash of the room ID (`windows::room_key`). Opening a room whose window exists focuses it.
- **Room-scoped events go only to their window** (`emit_to(label, …)`), named `room:<key>:…`. App-wide events (`sync:status`, `buddylist:update`, …) go to every window (`emit`).
- **Late windows catch up.** Rust keeps the latest value of every app-wide event in `Session` (status, buddies, recovery, trust, verification step). A window that opens later asks for it with a command. The frontend always **listens first, then asks**, so a change in between isn't lost (see [Frontend: the watch pattern](frontend.md#the-watch-pattern)).

## Data at rest

| What | Where | Notes |
|---|---|---|
| Saved session | OS keyring, service `bike.tyrel.aimtrix`, user `session` | JSON: homeserver, the store passphrase, and either a password session or an OAuth session with refresh token (`matrix/keyring.rs`). Never written to a file. |
| Matrix store | `<app data>/matrix-store/` (on Linux `~/.local/share/bike.tyrel.aimtrix/matrix-store/`) | matrix-sdk's SQLite stores (state, crypto, event cache, media), encrypted with the random passphrase kept in the keyring. A new sign-on always starts a fresh store, since a store belongs to one device. |
| Preferences | `<app config>/preferences.json` (on Linux `~/.config/bike.tyrel.aimtrix/`) | Not secret: auto-reply, text color, font, size, close to tray. Written atomically (temp file and rename). |
| Last server | The Sign On webview's `localStorage`, key `aimtrix.signon.server` | Only a convenience for the server picker. |

Secrets never leave Rust: tokens, the store passphrase and recovery keys are never sent to a window, and never logged. The one exception is by design: `setup_recovery` returns the new recovery key so Set Up Recovery can show it once.

## The main flows

### Launch

```
main.rs main()
  └─ (Linux) set WEBKIT_DISABLE_DMABUF_RENDERER=1 unless already set
  └─ aimtrix_lib::run()                                         lib.rs
       ├─ plugins: dialog, opener
       ├─ manage: matrix::Session, commands::BrowserSignOn, tray::Tray
       ├─ setup: manage preferences::Store, tray::create(), spawn startup()
       ├─ invoke_handler: the full command list
       └─ run loop (macOS: keep running with no windows; Dock click reopens)

startup()                                                       lib.rs
  └─ matrix::resume(data_dir)           keyring → restore client from store (works offline)
       ├─ Some(client) → session::begin(client) → windows::open_buddy_list()
       ├─ None         → windows::open_sign_on()
       └─ Err          → matrix::forget() → windows::open_sign_on()
```

### Sign on

```
signon/+page.svelte
  ├─ (typing pauses) invoke("sign_on_methods", {homeserver}) → {password, browser}
  └─ invoke("login", {homeserver, username, password})
     or invoke("login_with_browser", {homeserver})   (cancel_browser_login to stop)

commands::login / login_with_browser
  └─ matrix::sign_on / sign_on_with_browser      new store, check sliding sync, log in, save to keyring
  └─ session::begin(client)                      start sync and every watcher (below)
  └─ windows::open_buddy_list(); close the Sign On window
```

`session::begin` is where a signed-on session comes alive. It starts, in order:
1. `sync::start`: the `SyncService` (sliding sync, offline mode), reporting `sync:status`, saving refreshed OAuth tokens, and detecting a revoked token.
2. `presence::Tracker`: our presence heartbeat and buddy polling (servers may have no presence).
3. `rooms::watch`: the room list → `Vec<Buddy>` → `buddylist:update` (and the tray's unread count).
4. Presence → `presence:update` (and the tray's away dot).
5. `crypto::watch`: recovery state → `recovery:status`, opening Restore Messages once if this device needs the key.
6. `verify::watch_trust` → `verification:trust`; `verify::on_requests` opens Verify for incoming requests.
7. `auto_reply::watch`: answers IMs while away, if the preference is on.

All of it is stored in `Signed`, so `session::end` can stop each piece.

### Open an IM or Chat Room

```
+page.svelte (Buddy List)  invoke("open_room", {roomId})
commands::open_room        look the room up in Session.buddies (refuses invitations)
windows::open_room         focus existing room-<key>, or create it at /im?room=… or /chat?room=…
                           on Destroyed → session::unwatch_timeline()

im/ or chat/+page.svelte → Transcript.svelte
  watchTimeline(): listen room:<key>:timeline_diff, then invoke("room_timeline", {roomId})
commands::room_timeline → session::watch_timeline
  └─ timeline::watch     matrix-sdk Timeline → Vec<Line> now; each change → Diff → emit_to(room-<key>)
  └─ store Watching in Session.timelines; subscribe open rooms in sliding sync
chat/ also: watchChatRoom() → invoke("chat_room") → members::watch → room:<key>:members / :topic
```

Each timeline item maps to exactly one `Line`, so the window applies `Diff`s by index, the same way matrix-sdk's `VectorDiff`s apply to its items.

### Send a message

```
Compose.svelte   serialize the editor → {body, html|null}; clear the box at once
                 invoke("send_message", {roomId, body, html})
commands::send_message → session::send_message → timeline::Watching::send
                 html sanitized (matrix/html.rs), encrypted if the room is, sent via the Timeline
                 the local echo arrives straight away as a timeline diff; failure marks the line `failed`
```

Files go through `send_files` (paths, or an Open dialog), `send_pasted` (raw bytes in the request body) or `paste_picture` (Rust reads the clipboard). Uploads run as one task per room, reported as `room:<key>:upload` events.

### Sign off, or the server signs us out

```
Sign Off (Buddy List or tray) → invoke("logout") / tray menu → session::sign_off()
  └─ session::end()          stop every watcher, clear Session, close room and dialog windows
  └─ matrix::sign_off()      delete the device / revoke the OAuth token, forget keyring and store
  └─ open Sign On; close the Buddy List (destroy, even with "close to tray")

Token rejected (signed out elsewhere) → sync reports Status::SignedOut
  └─ session::signed_out()   end(), forget(), open Sign On with ?signed-out, close the Buddy List
```

## Platform notes

- **Window chrome:** every window is built with `decorations(false)`, and the frontend draws the AIM-style title bar and frame (`TitleBar.svelte`, `.aim-frame`). `windows.rs` adds `FRAME_WIDTH`/`FRAME_HEIGHT` to each content size.
- **Linux:** `main.rs` turns off WebKitGTK's DMA-BUF renderer, which often leaves covered windows black. The tray needs AppIndicator, which is checked with `libloading` before use (`tray::available`).
- **macOS:** closing the last window keeps the app running and the Dock icon reopens the Buddy List or Sign On (`lib.rs` run loop). Unread conversations badge the Dock icon (`tray::dock_badge`). There's no tray.
- **Windows:** has a tray, with "close to tray" on by default. Not tested yet (PLAN.md 8.3).
