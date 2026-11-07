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
- [x] **2.5 Buddy Icons.** In `media.rs`, fetch avatar thumbnails (authenticated media) and cache them on disk. *Done when:* the list shows icons, and rooms without one show a default.
- [x] **2.6 Invitations.** Put invites in their own group with Accept and Decline. *Done when:* accepting an invite moves the room into the right group.

### Phase 3: IM window
- [x] **3.1 Open window.** `open_room` creates or focuses a `room-<hash>` window titled with the buddy's name. *Done when:* double-clicking twice opens only one window.
- [x] **3.2 Read messages.** Subscribe to the room timeline and emit diffs. Render plain-text messages as "Name: text" in AIM colors. *Done when:* existing history shows up and new messages appear live.
- [x] **3.3 Send.** Add the text box: Enter sends, Shift+Enter or Alt+Enter adds a new line. *Done when:* the message shows up in Element.
- [x] **3.4 Scrollback.** Load older messages when you scroll up. *Done when:* you can reach the room's first message.
- [x] **3.5 Typing.** Send typing notices and show "Name is typing…" in the status bar. *Done when:* it works in both directions with Element.
- [x] **3.6 Formatting.** Add a B/I/U and font-color toolbar that sends `formatted_body`, and render incoming HTML safely (sanitized). *Done when:* formatting survives a round trip with Element, and a `<script>` tag gets stripped.
- [x] **3.7 Buddy icon pane and read receipts.** Show the buddy icon pane and mark the room read when the window has focus. *Done when:* the bold in the Buddy List clears after reading.
- [x] **3.8 Other message types.** Handle images (thumbnails), files, edits, deletions (redactions) and basic replies. *Done when:* each type sent from Element shows up sensibly.
- [x] **3.9 Save attachments.** Clicking a file or image in an IM window opens a Save dialog and saves it there, decrypted in encrypted rooms. *Done when:* a file and an image sent from Element can be saved and opened.

### Phase 4: Chat Room window
- [x] **4.1 Chat Room window.** Reuse the timeline component in a Chat Room layout for group rooms. *Done when:* a group opens as a Chat Room and a DM opens as an IM.
- [x] **4.2 Member list.** Add a member pane with display names and icons. *Done when:* it updates when someone joins.
- [x] **4.3 Room events.** Show "X has entered/left the room" lines and the topic. *Done when:* changes made in Element appear in the window.

### Phase 5: Encryption
- [x] **5.1 Encrypted rooms.** Confirm encrypted test rooms send and receive on this device. *Done when:* encrypted messages read and send both ways with Element.
- [x] **5.2 Unable to decrypt.** Show a clear placeholder for messages that can't be decrypted, and retry when keys arrive. *Done when:* the placeholder later turns into the real text.
- [x] **5.3 Recovery key.** Build a window that asks for the recovery key or passphrase on a new device. *Done when:* a fresh login can read old encrypted history.
- [x] **5.4 Emoji verification.** Build a Verify window for SAS emoji verification against another device. *Done when:* verifying with Element marks this device as trusted.
- [x] **5.5 New-account setup.** Set up key backup and recovery for accounts that have none, and show the recovery key once. *Done when:* the key from 5.5 works in 5.3 on another device.

### Phase 6: AIM extras
- [x] **6.1 Presence.** Detect whether the server has presence. If it does, split buddies into Online and Offline groups. *Done when:* both servers (presence on and off) look right.
- [x] **6.2 Away messages.** Set your own away message, and show buddies' away messages in a tooltip and the IM window. *Done when:* an away message set in AIMtrix shows up in Element.
- [x] **6.3 Add Buddy.** Start a new DM by entering a Matrix ID. *Done when:* the new DM appears under Buddies.
- [x] **6.4 Send files.** Send a file or image from an IM or Chat Room window: a button that opens a file picker (run from Rust, like Save), plus drag and drop onto the window. Images get a thumbnail and size; uploads are encrypted in encrypted rooms (the SDK's `send_attachment`), show progress in the status bar, and can be cancelled. *Done when:* a file and an image sent from AIMtrix open in Element, in a plain and an encrypted room.
- [x] **6.5 Block.** Block and unblock buddies (ignore user). *Done when:* a blocked user's messages are hidden.
- [x] **6.6 Preferences window.** Settings for auto-reply and fonts. *Done when:* settings persist across restarts.
- [x] **6.7 Auto-reply.** When away, auto-reply once per conversation. Off by default. *Done when:* it replies once, not on every message.
- [x] **6.8 Packaging.** Build Linux packages (AppImage/deb) with `npm run tauri build`. *Done when:* the package installs and runs on a clean machine.

### Phase 7: Other servers and sign-in
Beyond the dev Synapse: matrix.org (Synapse behind Matrix Authentication Service), other server software, and OAuth sign-on. Test against local servers only; matrix.org with a real account is a manual check you run yourself.
- [x] **7.1 Unsupported servers.** AIMtrix needs simplified sliding sync (MSC4186), and the SDK has no fallback to the older sync. Check for it at sign-on (`org.matrix.simplified_msc3575` in `/versions`, or a failed first sync) and explain, instead of showing a sync error. *Done when:* signing on to a server without it says so clearly, and nothing is left signed on.
- [x] **7.2 Another server implementation.** Add a continuwuity (conduwuit successor) container to `dev/` with presence on, seed it, and run the manual checklist and as many server tests as make sense against it. Log what breaks under Known issues. *Done when:* sign-on, Buddy List, IMs and encryption work on it, or each gap is logged.
- [ ] **7.3 Matrix Authentication Service in dev.** Put MAS in front of a dev Synapse, set up like matrix.org (auth delegated to MAS, password login still allowed through MAS's compatibility layer). *Done when:* password sign-on and the existing server tests pass against it, or the gaps are logged. Expect identity resets to need approval in the browser.
- [ ] **7.4 OAuth sign-on.** A "Sign on with browser" button using the SDK's OAuth support (authorization code with PKCE, dynamic client registration, a loopback redirect). Needed for accounts without a password, such as matrix.org accounts made with Google, GitHub or Apple. OAuth tokens expire, so save refreshed tokens to the keyring as they change, and revoke the token on sign-off. Offer it only when the server advertises OAuth (`/auth_metadata`). *Done when:* an account without a password on the dev MAS signs on in the browser, survives a restart and a token refresh, and signs off cleanly.
- [ ] **7.5 matrix.org check.** You sign on with your own matrix.org account, by password and by OAuth, and go through the manual checklist. Watch for slowness with many rooms and for rate limits. *Done when:* it works, or each problem is logged.

### Phase 8: Cross-platform
So far AIMtrix is built and tested only on Linux. The pieces are cross-platform (Tauri, matrix-sdk, `keyring` with the macOS Keychain and Windows Credential Manager, `arboard` for the clipboard, the dialog plugin), but nothing has been run elsewhere.
- [ ] **8.1 CI builds.** A GitHub Actions matrix (Linux, macOS, Windows) that runs `npm run check`, the Rust unit tests and a `tauri build`, plus the server tests on Linux against the dev Synapse in Docker. *Done when:* all three build and pass on every push.
- [ ] **8.2 macOS.** Run it on a Mac and go through the manual checklist. Watch for: the Keychain prompt and session resume; Cmd+C/Cmd+V/Cmd+A in the message box (macOS webviews need an Edit menu for these); Cmd+Q, Cmd+W and the app menu; closing the Buddy List vs. quitting (macOS apps usually keep running); file dialogs, drag and drop, and pasting pictures in WKWebView. *Done when:* the checklist passes on macOS, or each gap is logged.
- [ ] **8.3 Windows.** The same on Windows (WebView2): Credential Manager, Ctrl shortcuts, file paths with drive letters and backslashes in Send File and Save, drag and drop, pasting pictures. *Done when:* the checklist passes on Windows, or each gap is logged.
- [ ] **8.4 Look and feel per platform.** Decide between each platform's own title bars and AIM-style ones drawn by AIMtrix (`decorations: false` plus the `aim-titlebar` styles). Check the UI font falls back sensibly where Tahoma and MS Sans Serif are missing, and that shortcuts and hints say Cmd on macOS. *Done when:* windows look like AIMtrix on all three, with the choice written down here.
- [ ] **8.5 macOS and Windows packages.** `.dmg` and `.msi`/NSIS installers from `tauri build`, an icon per platform, and code signing (macOS needs an Apple Developer account and notarization, or Gatekeeper blocks the app; unsigned Windows installers get a SmartScreen warning). *Done when:* the packages install and run on a clean Mac and a clean Windows machine.

### Phase 9: Sounds
- [ ] **9.1 Sounds.** Add original sounds for new messages and sign-on/off, plus a mute toggle and a Preferences setting. *Done when:* sounds play and mute works.

## Risks and decisions to make during the build
- **Presence disabled on many homeservers:** if presence is off, put DMs in "Buddies" ordered by recent activity rather than splitting Online/Offline. Use real online/offline groups only when the server supports presence.
- **DM detection is messy:** `m.direct` can be stale. Treat it as a DM if it is in `m.direct` OR has ≤2 joined+invited members. Allow a manual override via a room tag such as `aimtrix.chat`.
- **Trademark and copyright:** recreate the *style* (Win9x/XP chrome, buddy-list layout). Do not ship AOL logos, the running-man mascot or the original sound files. Use original art and sounds.
- **Media:** newer servers require authenticated media (Matrix 1.11). `matrix-sdk`'s media API handles this. Cache thumbnails on disk.

## Known issues and follow-ups
Not tied to a step. Tick or remove them as they're dealt with.

- [ ] **Stale test devices.** Tests that fail or are killed before their cleanup leave devices behind; by 5.5 alice had 55. Every encrypted message is encrypted for each one, which slowed the encryption tests until they timed out. `python3 dev/clean-devices.py` removes "AIMtrix" devices not seen for an hour (removing 76 made the suite pass again, 25% faster). Still to do: have tests log out even when they fail.
- [ ] **Slow test logins.** Each login in a test takes about 5 s. The cause isn't confirmed; probably encryption and store setup in debug builds. Try `[profile.dev.package."*"] opt-level = 2` in `src-tauri/Cargo.toml` (slower first build).
- [ ] **Buddy List may miss tag and `m.direct` changes.** Suspected, untested: adding or removing `aimtrix.chat`, or an `m.direct` change, may not move the room until something else in it changes.
- [ ] **Recheck the SDK stop workaround when upgrading matrix-sdk.** In 0.19 a `SyncService::stop()` right after `start()` can lose the stop request and hang forever. `Syncing::stop` in `src-tauri/src/matrix/sync.rs` repeats the request every 100 ms. Drop the workaround once the SDK fixes this.
- [ ] **Script text shows in messages.** An incoming `<script>` tag is stripped, but its text (e.g. `alert(1)`) shows as plain text: matrix-sdk-ui's own cleaner drops the tag and keeps the text before ours runs. Harmless. To drop it, clean the raw event's `formatted_body` ourselves (`latest_json()`), minding edits and local echoes.
- [ ] **No full-size image viewer.** Images show a thumbnail; 3.9 makes them savable, but there's no way to view one full size in the app.
- [ ] **Videos and audio show as files.** No thumbnail or player yet.
- [ ] **Slash commands are sent as text.** Typing `/me`, `/invite @bob:localhost`, `/topic …` etc. in AIMtrix sends it as a plain message. Handle at least `/me` (emote), `/topic` and `/invite`, and refuse unknown commands instead of sending them (`//` to send a literal slash).
- [ ] **Dev-only "failed to load virtual css module" warning.** `npm run tauri dev` sometimes logs `[vite-plugin-svelte:load] failed to load virtual css module …/signon/+page.svelte?svelte&type=style&lang.css`. It happens when a window asks for a component's styles before Vite has compiled that component in the current dev server (a fresh `vite dev` plus a direct request for the `?svelte&type=style` URL reproduces it). Vite then serves the raw `.svelte` file as CSS, so that window may be unstyled until it reloads. What makes the app request styles first isn't confirmed. Production builds bundle CSS and aren't affected. If it keeps happening, note what was going on (sign off/on, window reopened, edit in progress).
- [ ] **Resetting the identity while syncing loses the new cross-signing keys.** `recovery().reset_identity()` makes new cross-signing keys locally, but uploading them needs the password (a second request through the returned handle). Meanwhile sync still sees the old identity on the server, and the SDK throws the new private keys away. The upload then goes through, but this device no longer has the keys, so recovery stays "Incomplete" and the recovery key it makes can't restore cross-signing. The 5.3 test sets bob up on a sign-on that isn't syncing (`ensure_bob_recovery`). 5.5 avoids it by never resetting: accounts whose identity this device can't use are asked to verify first. Any future "reset identity" option must pause sync during the reset. Consider reporting it upstream.
- [ ] **Server tests time out with too many threads.** With Phase 5's encryption tests (more logins), the default 16 threads overload the dev Synapse: about 1 in 3 runs has a random test time out (a dm-plain message missing from the open timeline, or a room key arriving too late). With `--test-threads=8` (now in CLAUDE.md) runs pass and take just as long. Encryption tests also wait until the sender knows the receiver's new device (`knows_device`) before sending. Longer term: give dm-plain tests their own temporary rooms, and clean up stale test devices (see above).
- [ ] **A test run hung once.** During 5.5, a full run stalled with `encrypted_rooms_send_and_receive_both_ways` and `undecryptable_messages_turn_into_text_when_keys_arrive` (both using carol in the encrypted DM) running for 8+ minutes instead of timing out, so some step awaits without a deadline. It didn't happen again, including with each step wrapped in a timeout. If it comes back, wrap the steps again (sign-on, send, sync stop, logout) to find the one that hangs.
- [ ] **Can't verify other people.** 5.4 only verifies your own sessions. Requests from other users (sent in a DM room) are ignored, and there's no way to verify a buddy from AIMtrix.
- [ ] **Presence is polled, and AIMtrix never goes idle.** Sliding sync carries no presence and doesn't mark us online, so `matrix/presence.rs` sets us online every 20 s (Synapse signs a device off after 30 s) and asks the server for each DM buddy's presence just as often. Changes show up to 20 s late, and each Buddy List costs one request per DM buddy every 20 s. The heartbeat also keeps us "online" while the computer sits unused; an auto-away after some idle time would fit 6.2 or 6.6. Quitting without signing off leaves us online for about 30 s. Synapse also takes only one presence update per user every 10 s by default, so going away right after signing on (or away and back quickly) can wait up to 10 s while the SDK retries.
- [ ] **Another online session cancels away.** Synapse combines presence across your devices, and online beats away. While Element (or any client that syncs as online) is open on another device, buddies see you as available, though your away message text is still set. AIMtrix can't override that. It could at least say so on the Buddy List, e.g. "Another session of yours is online, so buddies see you as available.", by reading its own presence back.
- [ ] **Element Web doesn't show away messages.** It shows the buddy as "Away" (presence `unavailable`) but never displays `status_msg`, the text of the away message; other AIMtrix users see it in the tooltip and IM window. Other clients may differ.
- [ ] **matrix.org's session limit.** matrix.org refuses to sign on an account that has too many sessions ("You have reached your hard device limit"), with `M_FORBIDDEN`, the same code as a wrong password. Sign On used to say "Incorrect screen name or password." for every `M_FORBIDDEN`; it now shows the server's own message unless it's about the password, and logs the server's reason. Still to do: link to the account page (`account_management_uri` in the server's `/auth_metadata`) so you can sign sessions out from there.
- [x] **The presence test timed out.** Fixed in 6.7. Every Buddy List test runs a presence tracker as alice, and Synapse takes one presence update per user every 10 s, so alice's presence check was rate-limited past the test's wait. The test now watches from a throwaway user. Tests that check presence or encryption with new accounts should use throwaway users too; `knows_device` now asks the server for devices (`/keys/query`) instead of waiting for sync.
- [ ] **Sending files: gaps.** A file being sent shows only in the status bar; its message appears once the upload is done (the SDK's progress reporting only works without its send queue, which also means no local echo, and no retry if the connection drops mid-upload). Cancelling after the upload finished may still send the message. Pasting sends one picture per paste. WebKitGTK doesn't hand pasted pictures to the page, so when a paste has no text, Rust reads the system clipboard itself (`arboard`); a paste with both text and a picture (some browsers' Copy Image) pastes the text. Videos and audio go as plain files, without a thumbnail or duration.
- [ ] **Blocking: gaps.** You can block only from an IM window. There's no Block in a Chat Room's member list or on the Buddy List itself, and no list of everyone you've blocked (someone blocked in a group shows nowhere). Their messages are hidden in Chat Rooms too, but they still show in the member list. Blocking or unblocking makes the SDK empty every conversation, so all open windows reload their last page of history.
- [x] **Windows go black where they were covered (Linux).** Fixed: on i3 (no compositor) with two AMD GPUs, WebKitGTK's DMA-BUF renderer didn't redraw areas a covered window exposed. `main.rs` now sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` on Linux unless the user set it. If it ever shows up anyway, `WEBKIT_DISABLE_COMPOSITING_MODE=1` is the heavier fallback.
- [ ] **Release build numbers restart daily.** `0.0.1-<seconds since midnight UTC>` (chosen until going public) isn't increasing across days, so a package manager may treat a newer build as older. Switch to a real version scheme when going public.
- [ ] **Links in messages don't open.** Clicking one does nothing (so it can't navigate the window away). Open them in the browser with the Tauri opener plugin.
- [x] **Flaky timeline tests under the full parallel run.** Fixed in 3.9. Two causes: the room list syncs only 1 event per room, so a burst of messages reset open conversations to the newest few (rooms with an open window are now subscribed, getting up to 20 per sync); and tests found Bob's DM by name, sometimes picking another test's temporary room (they now find seeded rooms by their `aimtrix.seed` marker).
- **Killed test runs leave debris.** A test stopped before its cleanup leaves rooms and invites on the test accounts, and tests that look rooms up by name can then pick the wrong one. Find rooms by ID or by the seed marker (`seed_room` in the tests), not by name, and clean up the accounts after killing a run.
- **Dev server rate limits.** Synapse's newer `rc_room_creation` limit made room-creating tests stall for minutes. `dev/synapse/homeserver.yaml` now relaxes it with the other limits; relax any new limit Synapse adds the same way.
- [ ] **continuwuity: reply quotes can show the username.** A reply to an old message quotes it as "bob" instead of "Bob Brown". continuwuity ignores `$LAZY`/`$ME` for `m.room.member` in sliding sync's `required_state` and sends member changes only in the timeline, which the SDK doesn't store (MSC4186 says `required_state` carries them). The replied-to message is fetched before that person's member event reaches the store, and the SDK doesn't refresh the quote's sender afterwards. `replies_to_old_messages_load_their_preview` fails on continuwuity for this. Member lists and room keys are handled (`sync::notice_member_changes` and the count check in `members::list` reload members from the server); this is what's left. Could look the name up ourselves when the quote's profile isn't ready, or report it to continuwuity.
- [ ] **continuwuity: encryption tests are slower and sometimes time out.** About 1 run in 4, a new account's first device takes ~34 s instead of ~4 s to become verified, past `new_accounts_get_a_recovery_key_that_restores_elsewhere`'s 30 s wait (with a longer wait it always gets there). Under the full parallel run, `emoji_verification_makes_a_new_device_trusted` sometimes doesn't see the verification request within its timeout; it passes run alone. Both pass on Synapse.

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
