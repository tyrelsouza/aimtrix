# Bug and security review, 2026-11-08

A review of the whole app, split by kind of bug: web and IPC security, secrets and crypto, unused code, wrong calculations, concurrency and lifecycle, Rust ↔ TypeScript contracts, panics and edge cases, and build, CI and scripts. Each finding was checked against the code (and against library source where behaviour depended on it) before fixing. What's fixed is summarized for users in `CHANGELOG.md` (Phase 8, "Bug and security review"); what's left is under "Known issues" in `PLAN.md`.

## Summary

No critical security holes: message HTML is sanitized tightly, no token, key, passphrase or message text reaches logs, windows or CI, and every Rust ↔ TypeScript payload matched its mirror. The review did find real ways to **lose encrypted history**, **crash or freeze the app**, and a few **security hardening gaps**, plus a set of wrong results and build-script bugs. All the significant ones are fixed; a handful of larger items are left, listed at the end.

## Fixed

### Keeping your encrypted history

| Problem | Fix |
|---|---|
| Any error while resuming (a locked or not-yet-running keyring, a full disk) deleted the saved session and its store, losing keys to old encrypted messages and leaving the device signed on at the server. | `startup()` keeps the session and opens Sign On with the reason (`?resume-failed=…`). Only an unreadable keyring entry is forgotten. Signing on again replaces it. |
| A slow sign-on or sign-off finishing late could delete the current session's store (every session shared `matrix-store/`), e.g. signing off while offline, then signing on again; or a stale browser sign-on timing out. | Each sign-on gets its own store folder, `stores/<random name>`, named in the saved session. `forget` only removes the keyring entry if it's still that device's (`keyring::forget_device`) and only that session's store. Closing Sign On cancels a waiting browser sign-on. |
| `session::begin` replaced a running session without ending it, leaving the old one's tasks running. | `begin(app, Start)` holds the session lock throughout, saves new sessions itself, and refuses (logging the new one out) if a session is already running. |
| Saving to the keyring after a successful login could fail, leaving a live device on the server each attempt (common on Linux without a Secret Service). | `NewSession::save` failure → `NewSession::discard` (logout, delete store), with a hint about GNOME Keyring or KWallet. |
| Sign Off could get stuck on the Buddy List if forgetting the session failed. | `session::sign_off` always ends at Sign On; errors are logged. |
| Two copies of AIMtrix could run on one store and one device, corrupting encryption state. | `tauri-plugin-single-instance`: a second launch shows the running one. |

### Crashes and freezes

| Problem | Fix |
|---|---|
| On Windows, a message with a far-future `origin_server_ts` overflowed `SystemTime` and crashed the app (release builds abort on panic), on every launch, whether or not you were away. | `checked_add`; an impossible time counts as old. |
| Auto-reply waited up to 60 s per undecryptable IM inside the event handler, which matrix-sdk awaits before syncing on, and polled the server every second. Sign-off could hang behind it. | Handlers only start tasks in a `JoinSet` (aborted on stop); waits back off 1, 2, 4… s, at most 8 at once. |
| Window-creating commands were synchronous and the tray created windows on the main thread: documented to deadlock on Windows. | Those commands are `async`; tray and Dock handlers spawn window work. |
| One network or server error while detecting presence turned presence off for the whole session. | Only a 403 means "no presence"; other errors retry. |
| A revoked token waited behind a 5 s sleep after a sync error. | The restart is a timer in the `select!`. |

### Security

| Problem | Fix |
|---|---|
| No Content Security Policy, and no navigation guard: a future HTML bug would have had full access to every command (e.g. `send_files` with any local path). | `aim_window` refuses navigation to anything but the app's pages and all new-window requests; a CSP allows scripts only from the app and images only from the app or `data:`. |
| Over plain http (reachable by a `.well-known` downgrade), a malicious server's OAuth metadata could make AIMtrix open any URL scheme (`smb:`, `search-ms:`, custom protocol handlers). | `check_browser_url`: https, or http on loopback only. |
| Image previews downloaded sender-chosen thumbnails, or encrypted images without a declared size, whole into memory: anyone in a room could exhaust memory. | Unencrypted images always use a thumbnail our own server makes; encrypted ones need a declared size within limits (2 MB thumbnail, 10 MB image); the downloaded length is checked again. |
| Attachment names kept control characters and bidi overrides (`invoice‮fdp.exe` looks like a `.pdf`) and Windows-forbidden characters. | `safe_file_name` strips them and trailing dots and spaces. |
| Auto-reply could send the away message into any room listed in `m.direct`, however many people were in it, and to blocked users. | Only rooms with at most two members, never ignored senders. |
| New store folders were created with the default umask. | Created `0700` on Unix. |

### Wrong results

- **Photos sent sideways:** EXIF orientation was ignored for dimensions and thumbnails. Now applied.
- **Upload progress jumped:** matrix-sdk reports a picture's thumbnail and file through one callback. Progress follows the biggest total.
- **A DM someone left still showed them** (presence, away message, polling). A joined DM with only you left has no other person. (A first version of this fix read each member from the store on every Buddy List rebuild and slowed the whole app; it now uses the in-memory member count.)
- **Add Buddy:** "Bob" now means `@bob` (Matrix IDs are lowercase); an existing two-person room tagged as a Chat Room isn't reused as the DM; a DM made moments ago opens instead of "That room isn't on your Buddy List."
- **Group invitations showed the DM icon:** invites usually have no member count; only the invite's "direct" flag decides.
- **One bad preference reset all of them.** Now each unreadable value falls back on its own.
- **File sizes:** Rust and TypeScript formatted differently ("512 bytes"/"512 B", no GB in TS) and rounded into "1024 KB". One rule, both sides.
- **Away message limit:** the textarea counted an emoji as two characters; it now counts like Rust.
- **Buddy List groups** guessed whether the server has presence from the buddies; now uses the server's answer.
- **Buddy Icons** that failed once never retried; now retried after a minute.
- Reply previews now replace `\r` as well as `\n`.

### Races

- A reloaded room window could apply diffs from the replaced watch onto the new snapshot. Each watch is numbered (`room_timeline` returns `{watch, lines}`; diffs carry `watch`), and the old one is stopped first.
- A window closed while its watch was starting left the watch (and its room subscription) running. Now dropped. Closing a window also clears your typing notice.
- `send_files` could start an upload for a window that had just closed. Checked.
- `subscribe_open_rooms` could apply an older set of rooms last. Serialized.
- Steps from an earlier verification could leak into the current one. Verifications are numbered; others' steps are dropped.
- Frontend watch helpers asked before their listener was in place and could let an older answer overwrite a newer event. One shared `watchValue` helper does it right.
- Tray updates could draw out of order. It draws the latest state.
- Pasted pictures were confirmed before checking the conversation was open.

### Build, release and scripts

- `release.sh` and the release workflow now require a commit on `main` (which CI checks), validate the tag, can be re-run (existing draft kept, uploads `--clobber`), and build `--locked`. `dev/set-version.mjs` updates aimtrix's entry in `Cargo.lock` so `--locked` works with stamped versions, and validates full SemVer.
- `test-package.sh` exited silently with `pipefail` when only one location had a `.deb`.
- `build-dmg.sh` copied stale `.dmg` files from earlier builds.
- `appimage-notices.sh` deleted the AppImage before repacking and hid the tool's errors.
- `third-party-licenses.mjs` broke on paths with spaces and on packages without a `license` field.
- `old-server.py` listened on every interface.
- `package-lock.json` had a stale root version.

### Docs and small cleanups

- Docs that didn't match the code (`docs/`, `README.md`, `CLAUDE.md`): a nonexistent Compose prop, "read receipts" and sendable fonts in the README, test coverage claims, enum casing, which state late windows read.
- Unused pieces: `Member.own` now marks "(you)" in Chat Room member lists; `Line::Message.sender` is the hover text on names; `.aim-sep` and `.aim-menubar__item.is-open` are shown in `/demo`; an orphaned doc comment removed.
- `Methods`, `ChatRoom` and `Emoji` got `rename_all = "camelCase"` so a future two-word field can't slip through as snake_case.
- Clippy runs in `mise run check` and CI with `-D warnings`.

## Not fixed (in `PLAN.md`, "Known issues")

Decisions for the user:
- **Version numbers restart daily** (`0.0.1-<seconds since midnight UTC>`, as chosen). Also hits macOS bundle version and Windows `.msi` limits later.
- **`productName` is lowercase**, so macOS shows "aimtrix"; changing it renames the Linux binary and `.deb` too.
- **CI isn't pinned:** newest stable Rust with `clippy -D warnings`, `:latest` dev server images, actions pinned by tag.

Larger work:
- **Soft logout** still forgets the session; needs a "sign in again on the same device" flow.
- **Encrypted images whose sender lies about the size** can still be downloaded up to their server's upload limit before being refused; a real cap needs a streaming download outside the SDK.
- **Refreshed OAuth tokens** are saved just after the SDK starts using them; use `Client::set_session_callbacks`.
- **Requests to a struggling server can hang for minutes** (matrix-sdk retries); password sign-on has no Cancel.
- **A failed resume leaves the old device on the server** when you sign on again.
- **The recovery key stays on the clipboard** after Copy.
- **Failed thumbnails and reply quotes aren't retried** until the window reopens.
- **Dev servers report ready too early** (no healthchecks for continuwuity and MAS), and an interrupted seed isn't repaired by re-running.

Reviewed and fine (not changed): the HTML sanitizer, `{@html}` use, data-URL image sniffing, CSS from preferences, `open_license`, capabilities, OAuth PKCE/state/revocation, keyring backend (no file fallback), store passphrase entropy, logging, verification only from your own sessions, mutex use, timeline diff ordering, and every IPC command, argument, payload and event name.

## Verification

- `npm run check`: 0 errors, 0 warnings. `cargo clippy --all-targets -- -D warnings`: clean (Linux). `node dev/third-party-licenses.mjs --check`: up to date (adds `tauri-plugin-single-instance`, MIT/Apache-2.0).
- Unit tests: 69 pass; new ones cover the OAuth URL check, the navigation guard, attachment names, turned photos, preference fallback, size formatting, bare-name lowercasing and invitations without a member count.
- Server tests, all four dev profiles: 105/105, twice, ~53 s (the same as before the changes; the first `other_person` fix had pushed it to 74 s and made a test fail).
- `undecryptable_messages_turn_into_text_when_keys_arrive` hung once (a known, intermittent issue); it now gives each step a named deadline, and passed 12 runs in a row.
- Package: built the `.deb` in the Ubuntu 22.04 container and ran `mise run test-package` on a clean Ubuntu 24.04: sign-on, the Buddy List and resuming render correctly under the new CSP (`dev/package-test/ubuntu-24.04/`).
- Not run: macOS and Windows (the Mac wasn't reachable; no Windows machine). CI builds and lints both on `main`.
