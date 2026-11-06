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
- **Frontend:** TypeScript with a small framework (Svelte or Solid) and hand-written CSS for Win98/XP-style window chrome. Each window type is its own route.
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

## Phases
1. **Scaffold and sign-on:** create a Tauri v2 app, add the `matrix-sdk` crate (features `sqlite`, `e2e-encryption`), and log in with a password. SSO/OIDC can come later. Save the session in the OS keyring and restore it on launch. Build the Sign On window.
2. **Buddy List:** use `RoomListService` (sliding sync) or the classic sync to build the list. Classify rooms as DMs or groups. Show groups, display names and avatar thumbnails. Collapse sections, show unread markers in bold, and open a room on double-click.
3. **IM window:** subscribe to the room's `Timeline` and render "ScreenName: message" lines with AIM-style colors (you in red, them in blue). Add a send box with basic formatting (B/I/U, font color) that sends `formatted_body`. Add back-pagination, a typing indicator in the status bar, and the buddy icon pane.
4. **Chat Room window:** the same timeline, plus a member list pane, join/leave lines ("X has entered/left the room") and room topic.
5. **E2EE:**
   - On first login, if key backup exists, ask for the recovery key or passphrase (`encryption().recovery().recover()`).
   - Offer SAS emoji verification against another of your devices in a Verify window.
   - Set up recovery for brand-new accounts.
   - Show a clear "unable to decrypt" placeholder that retries when keys arrive.
6. **AIM extras:** away messages (presence + `status_msg`, auto-reply optional and off by default), sounds (new message, sign-on/off, which require presence), Block (ignore user), "Add Buddy" (start a DM by MXID), invites in the Invitations group, warning-level easter egg optional.

## Risks and decisions to make during the build
- **Presence disabled on many homeservers:** if presence is off, put DMs in "Buddies" ordered by recent activity rather than splitting Online/Offline. Use real online/offline groups only when the server supports presence.
- **DM detection is messy:** `m.direct` can be stale. Treat it as a DM if it is in `m.direct` OR has ≤2 joined+invited members. Allow a manual override via a room tag such as `aimtrix.chat`.
- **Trademark and copyright:** recreate the *style* (Win9x/XP chrome, buddy-list layout). Do not ship AOL logos, the running-man mascot or the original sound files. Use original art and sounds.
- **Media:** newer servers require authenticated media (Matrix 1.11). `matrix-sdk`'s media API handles this. Cache thumbnails on disk.

## Verification
- **Local test homeserver:** run Synapse or conduwuit in Docker with presence enabled. Create 3 test users, a DM between 2 of them and a 3-person group, both encrypted.
- **Unit tests** (`cargo test`): room classification and grouping, plus timeline-item → display-line formatting.
- **Manual end-to-end** (`cargo tauri dev`):
  - Sign on, then check the buddy list groups and icons.
  - Open the DM and check it gets an IM window. Exchange messages with Element (web) as the other user, including formatting and typing.
  - Open the group and check it gets a Chat Room window with members.
  - Log in on a fresh device and restore with the recovery key; old encrypted history should decrypt.
  - Verify via emoji SAS with Element.
  - Restart the app and confirm the session restores without logging in again.
