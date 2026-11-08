# IPC reference

Windows talk to the Rust core in two ways:
- **Commands:** `invoke("name", { args })` from `@tauri-apps/api/core` calls a `#[tauri::command]` in `src-tauri/src/commands.rs` and returns a promise. Errors reject with a user-readable string.
- **Events:** `listen("name", handler)` from `@tauri-apps/api/event` receives what Rust sends with `app.emit` (every window) or `app.emit_to(label, …)` (one window).

Argument names are snake_case in Rust and camelCase in JS (`room_id` → `roomId`). Payload shapes follow serde: structs camelCase, enum values kebab-case. The TypeScript mirrors are in `src/lib/*.ts`.

`<key>` in event names is the room key (`windows::room_key`, the hash in the window's `room-<key>` label). Room windows get it from `roomKey()` in `src/lib/timeline.ts`.

## Commands

### Sign on and off

| Command | Args | Returns | Used by |
|---|---|---|---|
| `sign_on_methods` | `homeserver` | `{ password: boolean, browser: boolean }` | Sign On, as you type the server |
| `login` | `homeserver`, `username`, `password` | user ID. Also opens the Buddy List and closes Sign On. | Sign On |
| `login_with_browser` | `homeserver` | user ID, once the browser comes back. Same window changes as `login`. | Sign On |
| `cancel_browser_login` | | | Sign On |
| `logout` | | Signs off, opens Sign On, closes the Buddy List. | Buddy List |
| `whoami` | | user ID or `null` | Buddy List |

### Buddy List

| Command | Args | Returns | Used by |
|---|---|---|---|
| `sync_status` | | `SyncStatus` (later: `sync:status`) | Buddy List |
| `buddy_list` | | `Buddy[]` (later: `buddylist:update`) | Buddy List, IM, Chat Room |
| `accept_invite` / `decline_invite` | `roomId` | | Buddy List |
| `open_room` | `roomId` | Opens or focuses the room's window. Fails for invitations and rooms not on the list. | Buddy List, Add Buddy |
| `add_buddy` | `buddy` (Matrix ID, or a name on your server) | `{ roomId, existing }` | Add Buddy |
| `block_buddy` | `userId`, `name` | `false` if you said no to the confirmation | IM |
| `unblock_buddy` | `userId` | | IM |
| `my_icon` | | your Buddy Icon as a `data:` URL, or `null` | IM |
| `room_icon` | `roomId` | the room's Buddy Icon at 96 px, or `null` | IM |

### Conversations

These need the room's window to be open (its timeline watched), except `room_timeline` and `chat_room`, which start the watch.

| Command | Args | Returns | Notes |
|---|---|---|---|
| `room_timeline` | `roomId` | `Line[]` | Starts or replaces the watch; changes arrive as `room:<key>:timeline_diff`, typing as `room:<key>:typing`. |
| `chat_room` | `roomId` | `{ members: Member[], topic: string \| null }` | Changes arrive as `room:<key>:members` and `room:<key>:topic`. |
| `send_message` | `roomId`, `body`, `html` (or `null`) | | `html` is sanitized in Rust. The local echo arrives as a diff. |
| `load_older` | `roomId` | `true` once the room's start is reached | Older lines arrive as diffs. |
| `set_typing` | `roomId`, `typing` | | |
| `mark_read` | `roomId` | | The Buddy List un-bolds the room on its next update. |
| `image_thumbnail` | `roomId`, `eventId` | `data:` URL | |
| `save_attachment` | `roomId`, `eventId` | saved file name, or `null` if you cancelled the Save dialog | Rust shows the dialog. |
| `send_files` | `roomId`, `paths` (or `null` for an Open dialog) | `false` if you cancelled the dialog | Progress arrives as `room:<key>:upload`. |
| `send_pasted` | raw bytes as the body; headers `x-room-id`, `x-mime-type` | `false` if you said no | Called as `invoke("send_pasted", bytes, { headers })` (`conversation/upload.ts`). |
| `paste_picture` | `roomId` | `false` if there's no picture or you said no | Rust reads the system clipboard. |
| `cancel_upload` | `roomId` | | A file already uploaded may still arrive. |

### Presence and preferences

| Command | Args | Returns | Used by |
|---|---|---|---|
| `own_presence` | | `{ supported: boolean \| null, away: string \| null }` (later: `presence:update`) | Buddy List, Away Message |
| `set_away` | `message` (or `null` to come back) | | Away Message, Buddy List |
| `get_preferences` | | `Preferences` (later: `preferences:update`) | Preferences, Compose, Transcript, Away Message |
| `set_preferences` | `preferences` | the preferences as saved (out-of-range values reset) | Preferences |
| `tray_available` | | `boolean` | Preferences |

### Encryption

| Command | Args | Returns | Used by |
|---|---|---|---|
| `recovery_status` | | `Recovery` (later: `recovery:status`) | Buddy List, Restore Messages |
| `recover` | `secret` (recovery key or passphrase) | | Restore Messages |
| `setup_recovery_check` | | `"ready" \| "verify-first" \| "already-set-up"` | Set Up Recovery |
| `setup_recovery` | | the new recovery key (the only time it leaves Rust) | Set Up Recovery |
| `device_trust` | | `Trust` (later: `verification:trust`) | Buddy List |
| `verify_device` | | Asks your other sessions and opens Verify. | Buddy List, Set Up Recovery |
| `verification_step` | | `Step` or `null` (later: `verification:step`) | Verify |
| `accept_verification` | | | Verify |
| `start_verification` | | | Verify |
| `confirm_verification` | `matched` | | Verify |

There's no cancel command: Cancel and Decline close the Verify window, and closing it cancels an unfinished verification on both sides (`session::end_verification`).

### Opening windows

| Command | Opens | Args |
|---|---|---|
| `open_add_buddy` | Add Buddy | |
| `open_away` | Away Message | |
| `open_preferences` | Preferences | |
| `open_about` | About AIMtrix | |
| `open_recovery` | Restore Messages | |
| `open_setup_recovery` | Set Up Recovery | |
| `open_license` | AIMtrix's license or the third-party licenses, in the default viewer | `which`: `"license"` or `"third-party"` |

Each one focuses the window if it's already open. Rust also opens windows by itself: the Buddy List and Sign On (sign-on, sign-off, startup), Restore Messages (once, when this device needs the key) and Verify (on an incoming request).

## Events

### To every window

| Event | Payload | Sent when | Read first with |
|---|---|---|---|
| `sync:status` | `{ state: "connecting" \| "online" \| "offline" \| "signed-out" }` or `{ state: "error", message }` | Connection changes | `sync_status` |
| `buddylist:update` | `Buddy[]` (the whole list, most recent first) | Rooms change, an icon arrives, presence changes, someone is blocked | `buddy_list` |
| `presence:update` | `{ supported, away }` | Support is found out, or you go away or come back | `own_presence` |
| `recovery:status` | `"unknown" \| "enabled" \| "disabled" \| "incomplete"` | Recovery state changes | `recovery_status` |
| `verification:trust` | `"unknown" \| "no-cross-signing" \| "verified" \| "unverified"` | This device's trust changes | `device_trust` |
| `preferences:update` | `Preferences` | Preferences are saved | `get_preferences` |

### To one window

| Event | Window | Payload |
|---|---|---|
| `verification:step` | `verify` | `Step`, tagged by `step`: `waiting`, `incoming {device}`, `starting {device, canStart}`, `compare {device, emojis: [{symbol, description}]}`, `confirmed {device}`, `done {device}`, `cancelled {reason}` |
| `room:<key>:timeline_diff` | `room-<key>` | `Diff`, tagged by `op`: `append {values}`, `clear`, `pushFront {value}`, `pushBack {value}`, `popFront`, `popBack`, `insert {index, value}`, `set {index, value}`, `remove {index}`, `truncate {length}`, `reset {values}` |
| `room:<key>:typing` | `room-<key>` | `string[]`: display names of who's typing (never you) |
| `room:<key>:members` | `room-<key>` (Chat Room) | `Member[]`, the whole list sorted by name: `{ userId, name, own, icon }` |
| `room:<key>:topic` | `room-<key>` (Chat Room) | `string \| null` |
| `room:<key>:upload` | `room-<key>` | `Upload`, tagged by `state`: `sending {name, index, count, sent, total}`, `done`, `failed {message}`, `cancelled` |

## Payload types

The authoritative definitions are the Rust types; the TypeScript mirrors are listed for convenience.

| Type | Rust | TypeScript |
|---|---|---|
| `SyncStatus` | `matrix/sync.rs` `Status` | `src/lib/sync.ts` |
| `Buddy`, `Section`, `Presence` | `matrix/rooms.rs`, `matrix/presence.rs` | `src/lib/buddylist.ts` |
| `Line`, `Diff`, `Media`, `Reply`, `UndecryptableReason`, `MemberChange` | `matrix/timeline.rs` | `src/lib/timeline.ts` |
| `Member`, `ChatRoom` | `matrix/members.rs` | `src/lib/chatroom.ts` |
| `Own` | `matrix/presence.rs` | `src/lib/presence.ts` (`OwnPresence`) |
| `Recovery`, `Setup` | `matrix/crypto.rs` | `src/lib/recovery.ts`; `Setup` in `routes/setup-recovery` |
| `Trust`, `Step`, `Emoji` | `matrix/verify.rs` | `src/lib/verify.ts` |
| `Preferences`, `FontSize` | `preferences.rs` | `src/lib/preferences.ts` |
| `Upload` | `session.rs` | `src/lib/conversation/upload.ts` |
| `Methods` | `matrix/mod.rs` | `routes/signon` |
| `Added` | `matrix/rooms.rs` | `routes/add-buddy` |

### `Buddy`

```ts
{
  roomId: string;
  name: string;
  kind: "dm" | "group";
  section: "favorites" | "buddies" | "online" | "chats" | "invitations" | "offline";
  unread: boolean;
  icon: string | null;          // data: URL; null = default icon
  presence: "online" | "away" | "offline" | null;   // null: no presence on this server, or not a joined DM
  awayMessage: string | null;
  userId: string | null;        // the other person, for a DM
  blocked: boolean;
}
```

### `Line`

```ts
| { type: "message"; id; eventId: string | null; sender; name; own: boolean;
    body: string; html: string | null; emote: boolean; edited: boolean;
    media: { kind: "image"|"video"|"audio"|"file"; name; size; width; height } | null;
    reply: { name: string | null; preview: string | null } | null;
    failed: boolean; timestamp: number }
| { type: "deleted"; id; name; own; timestamp }
| { type: "undecryptable"; id; eventId; name; own; reason; timestamp }
| { type: "membership"; id; name; change: "joined"|"left"|"invited"|"kicked"|"banned"; timestamp }
| { type: "topic"; id; name; topic: string | null; timestamp }
| { type: "start"; id }
| { type: "other"; id }
```

`id` is the timeline item's unique ID, stable across updates (use it as the `{#each}` key). `eventId` is `null` for your own message until the server accepts it. `html` is already sanitized.
