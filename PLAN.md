# AIMtrix: a Matrix client with an AIM-style interface

## Context
`PLAN.md` describes a Matrix client that looks and behaves like AOL Instant Messenger:
- The Buddy List shows Matrix rooms.
- A 1:1 DM opens an AIM-style IM window (two people, back and forth).
- A room with several people opens an AIM-style Chat Room window.
- Buddy Icons are Matrix profile pictures.

The repo has only `PLAN.md` so far, so this is a new project. You chose a **Tauri desktop app** with **full E2EE in v1**.

## Verdict: feasible
Matrix maps onto AIM concepts cleanly:

| AIM | Matrix |
|---|---|
| Buddy List | Joined rooms (room list) |
| Buddy (IM) | DM room: in `m.direct` account data, or exactly 2 joined members |
| Chat Room | Any other room |
| Buddy Icon | User avatar (`mxc://`) for DMs, room avatar for groups |
| Screen name | Display name, with the MXID as fallback |
| Buddy groups | Room tags: `m.favourite` → "Favorites", DMs → "Buddies", groups → "Chats", invites → "Invitations" |
| Online/Away/Offline | Presence and `status_msg`. Many servers turn presence off, matrix.org included, so the UI needs a fallback (see Risks) |
| Away message | Set your own presence to `unavailable` plus a `status_msg` |
| "Buddy is typing…" | `m.typing` events |
| Rich text (fonts, colors) | `formatted_body` (`org.matrix.custom.html`). AIM used HTML too, so this matches well |
| Block | Ignore user (`m.ignored_user_list`) |
| "X has entered the room" | `m.room.member` join and leave events |
| Sign-on/off door sounds | Presence changes. Use original sound files, not AOL's |

**Hard parts:**
1. **E2EE:** device verification, restoring keys from backup, and handling "unable to decrypt".
2. **Multi-window state:** one Matrix session has to serve many Tauri windows.
3. **Presence:** often unavailable, as noted in the table.

None of these block the project. The architecture below handles all three.

## Architecture
**The Matrix client runs in the Rust core, using `matrix-sdk` (matrix-rust-sdk).** Each webview window is a thin UI layer.
- Why Rust rather than matrix-js-sdk in the webview: Tauri gives each window its own JS context. A JS client would have to live in one window and relay to the others, and its IndexedDB crypto store does not handle being shared. In the Rust core, a single `Client` (with an SQLite store and crypto) serves every window through Tauri commands and events. Element X uses the same SDK, so the E2EE, sliding sync, `RoomListService` and `Timeline` APIs are well tested.
- **Frontend:** SvelteKit (Svelte 5, TypeScript, static adapter in SPA mode) and hand-written CSS for Win98/XP-style window chrome. Each window type is its own route.
- **Windows:** the Buddy List is the main window. Each IM or Chat Room opens its own `WebviewWindow`, labeled `room-<roomId-hash>`, and opening an existing one focuses it. A Sign On window handles login.
- **IPC:**
  - Commands: `login`, `logout`, `open_room`, `send_message`, `send_typing`, `paginate_back`, `set_away`, `verify_*`, `recover_with_key`.
  - Events pushed to windows: `buddylist:update`, `room:<id>:timeline_diff`, `room:<id>:typing`, `presence:update`, `verification:*`.

### Proposed layout
```
AIMtrix/
  src-tauri/
    src/main.rs          # Tauri setup, window management
    src/matrix/mod.rs    # Client bootstrap, session restore (keyring + sqlite store)
    src/matrix/rooms.rs  # RoomListService -> BuddyList model (DM detection, grouping)
    src/matrix/timeline.rs # per-room Timeline subscription -> diffs to window
    src/matrix/crypto.rs # recovery key, SAS emoji verification, UTD handling
    src/matrix/media.rs  # avatar thumbnails (authenticated media) -> cached files / data URLs
    src/commands.rs      # #[tauri::command] surface
  src/                   # frontend
    windows/SignOn, BuddyList, IM, ChatRoom, Verify, AwayMessage
    styles/aim-chrome.css
    assets/sounds/       # original sounds only
```

## Steps
Work one step at a time, top to bottom. Each step is small enough for one sitting and ends with something you can run or test. When a step is finished, tick its box (`[x]`).

### Phase 0: Foundations
- [x] **0.1 Test homeserver.** Add `dev/docker-compose.yml` with Synapse (presence enabled) and a seed script that creates 3 users, a DM, and a 3-person group, each in an encrypted and an unencrypted version. *Done when:* Element Web can log in as each user and see the rooms.
- [x] **0.2 Scaffold.** Run `git init` and create a Tauri v2 app with a TypeScript + Svelte/Solid frontend. *Done when:* `npm run tauri dev` opens an empty window.
- [x] **0.3 AIM chrome CSS.** Build `src/styles/aim-chrome.css` with window frame, title bar, buttons, list, text box and status bar styles. *Done when:* a static demo page shows the pieces.

### Phase 1: Sign On
- [x] **1.1 First login.** Add `matrix-sdk` and a `login` command that takes homeserver, username and password. *Done when:* logging in as a test user returns the user ID (never log the token).
- [x] **1.2 Sign On window.** Build the form UI, wire it to `login`, and show errors. *Done when:* a bad password shows an error and a good one closes the window.
- [x] **1.3 Session restore.** Keep the session in the OS keyring and use the SQLite store. Restore the session on launch and add `logout`. *Done when:* restarting the app skips Sign On, and logout brings it back.
- [x] **1.4 Sync.** Start syncing after login and show connection state (connecting, online, error). If the server rejects the saved token (`M_UNKNOWN_TOKEN`, e.g. signed out from another client), forget the session and show Sign On. *Done when:* the status shows online and survives a network blip.

### Phase 2: Buddy List
- [x] **2.1 Room classification.** Write a pure function in `rooms.rs` that sorts rooms into DM vs. group and into Favorites/Buddies/Chats/Invitations, with unit tests. No UI. *Done when:* `cargo test` passes and covers stale `m.direct` and the `aimtrix.chat` override.
- [x] **2.2 Room list events.** Subscribe to the room list and emit `buddylist:update`. *Done when:* a plain-text list in the main window updates live as rooms change.
- [x] **2.3 Buddy List window.** Show collapsible groups with "(n)" counts and display names. *Done when:* it matches the test server's rooms.
- [x] **2.4 Unread and ordering.** Bold rooms with unread messages and sort by recent activity. *Done when:* a message sent from Element bolds the buddy and moves it up.
- [ ] **2.5 Buddy Icons.** In `media.rs`, fetch avatar thumbnails (authenticated media) and cache them on disk. *Done when:* the list shows icons, and rooms without one show a default.
- [ ] **2.6 Invitations.** Put invites in their own group with Accept and Decline. *Done when:* accepting an invite moves the room into the right group.

### Phase 3: IM window
- [ ] **3.1 Open window.** `open_room` creates or focuses a `room-<hash>` window titled with the buddy's name. *Done when:* double-clicking twice opens only one window.
- [ ] **3.2 Read messages.** Subscribe to the room timeline and emit diffs. Render plain-text messages as "Name: text" in AIM colors. *Done when:* existing history shows up and new messages appear live.
- [ ] **3.3 Send.** Add the text box: Enter sends, Shift+Enter adds a new line. *Done when:* the message shows up in Element.
- [ ] **3.4 Scrollback.** Load older messages when you scroll up. *Done when:* you can reach the room's first message.
- [ ] **3.5 Typing.** Send typing notices and show "Name is typing…" in the status bar. *Done when:* it works in both directions with Element.
- [ ] **3.6 Formatting.** Add a B/I/U and font-color toolbar that sends `formatted_body`, and render incoming HTML safely (sanitized). *Done when:* formatting survives a round trip with Element, and a `<script>` tag gets stripped.
- [ ] **3.7 Buddy icon pane and read receipts.** Show the buddy icon pane and mark the room read when the window has focus. *Done when:* the bold in the Buddy List clears after reading.
- [ ] **3.8 Other message types.** Handle images (thumbnails), files, edits, deletions (redactions) and basic replies. *Done when:* each type sent from Element shows up sensibly.

### Phase 4: Chat Room window
- [ ] **4.1 Chat Room window.** Reuse the timeline component in a Chat Room layout for group rooms. *Done when:* a group opens as a Chat Room and a DM opens as an IM.
- [ ] **4.2 Member list.** Add a member pane with display names and icons. *Done when:* it updates when someone joins.
- [ ] **4.3 Room events.** Show "X has entered/left the room" lines and the topic. *Done when:* changes made in Element appear in the window.

### Phase 5: Encryption
- [ ] **5.1 Encrypted rooms.** Confirm encrypted test rooms send and receive on this device. *Done when:* encrypted messages read and send both ways with Element.
- [ ] **5.2 Unable to decrypt.** Show a clear placeholder for messages that can't be decrypted, and retry when keys arrive. *Done when:* the placeholder later turns into the real text.
- [ ] **5.3 Recovery key.** Build a window that asks for the recovery key or passphrase on a new device. *Done when:* a fresh login can read old encrypted history.
- [ ] **5.4 Emoji verification.** Build a Verify window for SAS emoji verification against another device. *Done when:* verifying with Element marks this device as trusted.
- [ ] **5.5 New-account setup.** Set up key backup and recovery for accounts that have none, and show the recovery key once. *Done when:* the key from 5.5 works in 5.3 on another device.

### Phase 6: AIM extras
- [ ] **6.1 Presence.** Detect whether the server has presence. If it does, split buddies into Online and Offline groups. *Done when:* both servers (presence on and off) look right.
- [ ] **6.2 Away messages.** Set your own away message, and show buddies' away messages in a tooltip and the IM window. *Done when:* an away message set in AIMtrix shows up in Element.
- [ ] **6.3 Sounds.** Add original sounds for new messages and sign-on/off, plus a mute toggle. *Done when:* sounds play and mute works.
- [ ] **6.4 Add Buddy.** Start a new DM by entering a Matrix ID. *Done when:* the new DM appears under Buddies.
- [ ] **6.5 Block.** Block and unblock buddies (ignore user). *Done when:* a blocked user's messages are hidden.
- [ ] **6.6 Preferences window.** Settings for sounds, auto-reply and fonts. *Done when:* settings persist across restarts.
- [ ] **6.7 Auto-reply.** When away, auto-reply once per conversation. Off by default. *Done when:* it replies once, not on every message.
- [ ] **6.8 Packaging.** Build Linux packages (AppImage/deb) with `npm run tauri build`. *Done when:* the package installs and runs on a clean machine.

## Risks and decisions to make during the build
- **Presence disabled on many homeservers:** if presence is off, put DMs in "Buddies" ordered by recent activity rather than splitting Online/Offline. Use real online/offline groups only when the server supports presence.
- **DM detection is messy:** `m.direct` can be stale. Treat it as a DM if it is in `m.direct` OR has ≤2 joined+invited members. Allow a manual override via a room tag such as `aimtrix.chat`.
- **Trademark and copyright:** recreate the *style* (Win9x/XP chrome, buddy-list layout). Do not ship AOL logos, the running-man mascot or the original sound files. Use original art and sounds.
- **Media:** newer servers require authenticated media (Matrix 1.11). `matrix-sdk`'s media API handles this. Cache thumbnails on disk.

## Verification
- **Local test homeserver:** run Synapse or conduwuit in Docker with presence enabled. Create 3 test users, a DM between 2 of them and a 3-person group, both encrypted.
- **Unit tests** (`cargo test`): room classification and grouping, plus timeline-item → display-line formatting.
- **Manual end-to-end** (`npm run tauri dev`):
  - Sign on, then check the buddy list groups and icons.
  - Open the DM and check it gets an IM window. Exchange messages with Element (web) as the other user, including formatting and typing.
  - Open the group and check it gets a Chat Room window with members.
  - Log in on a fresh device and restore with the recovery key; old encrypted history should decrypt.
  - Verify via emoji SAS with Element.
  - Restart the app and confirm the session restores without logging in again.
