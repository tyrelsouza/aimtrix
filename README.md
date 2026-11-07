# AIMtrix

A [Matrix](https://matrix.org) chat client that looks and behaves like AOL Instant Messenger, circa 2000: a Buddy List, a separate window for each IM and Chat Room, Buddy Icons and grey Win98/XP chrome. Underneath is ordinary Matrix, with end-to-end encryption.

> AIMtrix recreates AIM's *style* only. It isn't affiliated with AOL, and all its art and sounds are original.

**Status:** early and in progress (test builds are numbered 0.0.1-…). It's a desktop app built with Tauri, developed and tested on Linux, with `.deb` and `.AppImage` packages for Ubuntu 22.04, Debian 12 and newer. Progress is tracked step by step in [`PLAN.md`](PLAN.md), and [`CHANGELOG.md`](CHANGELOG.md) says what each step added.

## What works

| AIM | AIMtrix on Matrix |
|---|---|
| Sign On | Password sign-on to any homeserver, or in your web browser where the server supports it (OAuth: matrix.org, for accounts made with Google, GitHub or Apple). Servers that can't work with AIMtrix are refused with an explanation. The session is kept in the OS keyring, so the next launch skips sign-on. |
| Buddy List | Your rooms, in groups: Favorites (`m.favourite`), Buddies (DMs), Chats (rooms with several people) and Invitations, most recent first. Unread rooms are bold. |
| Online / Offline | On servers with presence, Buddies splits into Online and Offline. Servers without presence (matrix.org, for one) keep a single Buddies group. |
| Away message | Set one from the Buddy List. Buddies' away messages show on hover and in the IM window. Needs a server with presence. |
| Auto-response | While you're away, answer each IM once with your away message (off by default; turn on in Preferences). |
| Add Buddy | Start a DM by Matrix ID, or just a name on your own server. |
| Send File | Send files and pictures: a button, drag and drop, or paste a picture. Encrypted in encrypted rooms. |
| Block | Block and unblock a buddy from their IM window; their messages are hidden everywhere. |
| Preferences | Font and size for conversations, and the color of your messages. |
| IM window | One window per DM, with Buddy Icons, rich text (bold, italic, fonts, colors), "is typing…", read receipts, images, files, edits, deletions and replies. |
| Chat Room window | Member list, topic, and "X has entered the room" lines. |
| Buddy Icon | Profile pictures, with original default icons. |

**Encryption** works the way Matrix users expect:
- Encrypted DMs and rooms send and receive.
- A message that can't be decrypted yet shows a placeholder, which turns into the text when the keys arrive.
- A recovery key restores old messages on a new device.
- Emoji verification works against your other sessions.
- Accounts without recovery can set it up.

Coming next (see `PLAN.md`): checking matrix.org with real accounts; macOS and Windows; and last, original sounds with mute.

What counts as a DM: a room listed in `m.direct`, or one with at most two members. Tag a room `aimtrix.chat` to make it open as a Chat Room anyway.

## Building and running

You need:
- **Rust** (stable, edition 2024) and **Node.js** with npm.
- The [Tauri v2 system dependencies](https://v2.tauri.app/start/prerequisites/) for your OS. On Linux that's WebKitGTK 4.1 and friends.
- On Linux, a running Secret Service keyring (GNOME Keyring, KWallet or similar) to store the session.

```sh
npm install          # also installs the Tauri CLI; no global cargo-tauri needed
npm run tauri dev    # run in development
npm run tauri build  # build packages for this machine
```

### Linux packages and releases

Every push runs CI (`.github/workflows/ci.yml`) on Linux: the checks, unit tests, a release build and the server tests against the dev homeservers. Pushes to `main` also build on macOS and Windows.

`mise run package` builds a `.deb` and an `.AppImage` in an Ubuntu 22.04 container (into `dist/`), so they run on Ubuntu 22.04, Debian 12 and anything newer. `mise run test-package` installs the `.deb` on a clean container and checks that it signs on and resumes. `mise run release` tags the current commit and pushes the tag, and GitHub Actions attaches both packages to a draft release. Until AIMtrix goes public, builds are numbered `0.0.1-<seconds since midnight UTC>`.

## Development

Develop and test against the local Matrix server in `dev/`, never real accounts. It needs Docker:

```sh
docker compose -f dev/docker-compose.yml up -d
python3 dev/seed.py
```

This starts Synapse at `http://localhost:8008` and Element Web at `http://localhost:8080`, to play the other side of a conversation. The seed script creates the users `alice`, `bob` and `carol` (password `aimtrix-test`) and the test rooms. Optional profiles add a Synapse with presence turned off (`http://localhost:8009`), continuwuity (`http://localhost:8011`) and a Synapse behind Matrix Authentication Service, like matrix.org (`http://localhost:8013`). See [`dev/README.md`](dev/README.md) for all of them and the seeded rooms.

Checks:

```sh
npm run check                                        # type-check the frontend
cargo test --manifest-path src-tauri/Cargo.toml      # Rust unit tests
cargo test --manifest-path src-tauri/Cargo.toml -- --include-ignored --test-threads=8
                                                     # also the tests that use both dev servers
```

If the server tests slow down, leftover test devices are the usual cause; `python3 dev/clean-devices.py` removes them.

In a development build, the Buddy List has a "Style demo" link to `/demo`, which shows every piece of the AIM look.

## How it's built

- **Rust core** (`src-tauri/`): a single [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) client with an encrypted SQLite store owns the whole Matrix session: sync, rooms, timelines, encryption. The Matrix code lives in `src-tauri/src/matrix/`.
- **Windows** (`src/`): SvelteKit (Svelte 5, TypeScript) with one route per window type and hand-written CSS in `src/styles/aim-chrome.css`. Windows hold no Matrix code. They call Tauri commands (`src-tauri/src/commands.rs`) and listen for events such as `buddylist:update` and `room:<id>:timeline_diff`.

[`PLAN.md`](PLAN.md) has the full design and how each AIM idea maps to Matrix. [`CLAUDE.md`](CLAUDE.md) has the working rules for the code.

## License

Closed source for now. All rights reserved.
