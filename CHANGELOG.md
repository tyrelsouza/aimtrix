# Changelog

What changed in each phase of `PLAN.md`, step by step. Open problems live under "Known issues and follow-ups" in `PLAN.md`.

## Phase 5: Encryption

### Changes
- **5.1 Encrypted rooms.** Messages in encrypted rooms send and read both ways with Element, in DMs and Chat Rooms. This worked with no app changes, since the SDK's end-to-end encryption was already on. Messages sent before this device signed on still can't be read (see 5.2 and 5.3).
  - New server test `encrypted_rooms_send_and_receive_both_ways`: two fresh AIMtrix devices exchange messages in the seeded "Carol Chen" DM and "Secret Chat", after checking that both rooms are encrypted.

## Phase 4: Chat Room window

### Changes
- **4.1 Chat Room window.** Group rooms now open in a Chat Room window (`/chat`, titled "*Name* - Chat Room", 540×400 to leave space for the member list). DMs still open as IMs. `windows::open_room` chooses the window from the room's DM/group kind.
  - The message list and the compose box (toolbar, editor, typing notices, Send) are now shared components in `src/lib/conversation/` (`Transcript.svelte`, `Compose.svelte`), used by both windows. The status bar text is in `status.ts`.
  - The IM window is now built from these components and should look the same as before.
  - The Chat Room in `/demo` now shows the formatting toolbar.
- **4.2 Member list.** The Chat Room has a member pane on the right with the count ("3 people here") and each member's Buddy Icon and display name, sorted by name. It updates live when someone joins or leaves, or changes their name or avatar.
  - `matrix/members.rs` loads the joined members and resends the whole list as a `room:<key>:members` event on every member event and whenever an icon finishes downloading. Windows fetch the first list with the `room_members` command.
  - The watch stops when the window closes or you sign off.
- **4.3 Room events.** The conversation shows system lines for room events: "*Name* has entered the room.", "has left the room.", "was invited.", "was removed from the room." and "was banned from the room.", plus "*Name* changed the topic to: …" and "removed the topic." The Chat Room shows the current topic in a bar at the top, updated live. The lines show in IM windows too, since both share the transcript.
  - `timeline.rs` has new `Membership` and `Topic` lines. Accepted invites count as entering; declined invites and knocks get no line.
  - The Chat Room watcher (`members.rs`) now follows the topic as well. The `room_members` command became `chat_room`, which returns members and topic; topic changes arrive as `room:<key>:topic` events. The frontend helper is `watchChatRoom` in `src/lib/chatroom.ts` (was `members.ts`).

## Phase 3: IM window

### Changes
- **3.1 Open window.** Double-clicking a buddy opens an IM window labelled `room-<key>`, where the key is an FNV-1a hash of the room ID (labels and event names can't contain `!`). Opening it again focuses the window and restores it if minimized. Invitations don't open.
- **3.2 Read messages.** `timeline::watch` subscribes to the room's timeline and sends `room:<key>:timeline_diff` events. Each timeline item becomes exactly one display line, so the window applies diffs by position. Messages show as "Name: text" in AIM colors, with display names loaded from the member list.
- **3.3 Send.** Added the message box: Enter sends, Shift+Enter or Alt+Enter adds a new line. Sent messages show up straight away (local echo); one the server rejects is marked "(not sent)" and its text comes back for another try. Emotes (`/me`) show as "* Name text".
  - Bug fixes: signing off could hang because of a stop race in the SDK's `SyncService` (`Syncing::stop` now re-sends the stop request every 100 ms), and the dev Synapse's room-creation rate limit (`rc_room_creation`) was relaxed. The "Known issues and follow-ups" section was added to `PLAN.md`.
- **3.4 Scrollback.** Older messages load as you scroll up (or until the window can scroll), keeping your reading position, until "This is the beginning of the conversation."
- **3.5 Typing.** Sends typing notices (renewed every 2 s, stopped after sending or 5 s idle) and shows "Name is typing…" in the status bar.
- **3.6 Formatting.** Bold, italic, underline and an 8-color text palette, typed in a contenteditable editor and sent as `formatted_body`. Pasted text arrives unformatted.
  - Incoming and outgoing HTML is cleaned in Rust (`matrix/html.rs`, ammonia): only Matrix's formatting tags are kept, colors must be `#rrggbb`, links must be http(s), mailto or matrix, and `<script>`/`<style>` are removed.
  - Matrix's `data-mx-color` is applied in the window, and links don't navigate it.
- **3.7 Buddy icon pane and read receipts.** The IM window shows the buddy's icon and yours at 96 px. A read receipt is sent while the window has focus (300 ms debounce), which clears the bold in the Buddy List.
- **3.8 Other message types.** Images show as thumbnails (downloaded and decrypted in encrypted rooms; full images over 10 MB are skipped). Files show with an icon and size. Edits show "(edited)", deletions show "(message deleted)", and replies show "In reply to *Name*: preview", loading the original message if needed.
- **3.9 Save attachments.** Clicking a file or image opens a native Save dialog (`tauri-plugin-dialog`, run from Rust) and saves the file there, decrypted when needed. The status bar shows "Saving…" and then "Saved *name*". Unsafe file names are cleaned up before saving.
  - Fixed flaky timeline tests: rooms with an open window now get a sliding-sync subscription (up to 20 events per sync instead of 1), so a burst of messages no longer resets an open conversation. Tests now find seeded rooms by their `aimtrix.seed` marker, not by name.

## Phase 2: Buddy List

### Changes
- **2.1 Room classification.** Pure functions in `matrix/rooms.rs` sort rooms into DM or group, and into Favorites, Buddies, Chats or Invitations, with unit tests. A room counts as a DM if it is in `m.direct` or has at most 2 joined+invited members; the `aimtrix.chat` tag forces a group. The tests cover stale `m.direct` lists.
- **2.2 Room list events.** Watches the SDK's room list and sends `buddylist:update` to windows whenever it changes.
- **2.3 Buddy List window.** Collapsible groups with "(n)" counts and display names.
- **2.4 Unread and ordering.** Rooms with unread messages are bold, and the list is sorted by most recent activity.
- **2.5 Buddy Icons.** Avatar thumbnails are downloaded with authenticated media and cached (on disk in the SDK store, and as `data:` URLs in memory). Rooms without an avatar get an original default icon for DMs or chats.
- **2.6 Invitations.** Invites have their own group with Accept and Decline. An accepted room moves into the right group.

## Phase 1: Sign On

### Changes
- **1.1 First login.** Added matrix-sdk and a `login` command (homeserver, username, password) that returns the user ID. Access tokens are never logged.
- **1.2 Sign On window.** The Sign On form, which shows errors and closes once you're signed on.
- **1.3 Session restore.** The session is saved in the OS keyring (`matrix/keyring.rs`), with an SQLite store per session. On launch the app resumes into the Buddy List or opens Sign On. Added `logout`. Windows are now created in code (`windows.rs`).
- **1.4 Sync.** Sync starts after sign-on, and the Buddy List shows the connection status (connecting, online, offline, error) and recovers on its own. If the server rejects the saved token, the session is forgotten and Sign On opens with an explanation.

## Phase 0: Foundations

### Changes
- **0.1 Test homeserver.** `dev/docker-compose.yml` runs Synapse (presence on) and Element Web. `dev/seed.py` creates alice, bob and carol, plain and encrypted DMs and groups ("Test Chat" with about 60 messages for scrollback, "Secret Chat"), and an invitation. Seeded rooms are marked with an `aimtrix.seed` state event, so re-running the script is safe.
- **0.2 Scaffold.** Tauri v2 app with a SvelteKit frontend (Svelte 5, TypeScript, static SPA).
- **0.3 AIM chrome CSS.** `src/styles/aim-chrome.css` styles the window frame, title bar, buttons, lists, text boxes and status bar in a Win98/XP look. `/demo` shows each piece. All art is original.
