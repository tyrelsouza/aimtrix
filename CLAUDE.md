# AIMtrix

A Matrix client that looks and behaves like AOL Instant Messenger. See `PLAN.md` for the full design, the AIM→Matrix mapping and the build phases.

## Workflow
- Work through the `## Steps` checklist in `PLAN.md` one step at a time, in order. Don't start the next step unless asked.
- A step is finished only when its "Done when" check passes. Then tick its box (`[x]`) in `PLAN.md`.
- If a step turns out bigger than expected, split it into sub-steps in `PLAN.md` instead of doing it all at once.

## Stack
- **Tauri v2** desktop app.
- **Rust core** (`src-tauri/`): runs a single `matrix-sdk` (matrix-rust-sdk) `Client` with an SQLite store and E2EE. All Matrix logic lives here.
- **Frontend** (`src/`): TypeScript with Svelte or Solid and hand-written Win98/XP-style CSS. Windows are thin views with no Matrix SDK in JS.

## Architecture rules
- One Matrix session, owned by the Rust core. Windows talk to it only through `#[tauri::command]`s (`src-tauri/src/commands.rs`) and Tauri events (`buddylist:update`, `room:<id>:timeline_diff`, `room:<id>:typing`, `presence:update`, `verification:*`).
- The Buddy List is the main window. Each IM or Chat Room gets its own `WebviewWindow`, labeled `room-<roomId-hash>`. Opening a room that already has a window focuses that window.
- DM vs. group: a room is a DM if it is in `m.direct` OR has ≤2 joined+invited members. The `aimtrix.chat` room tag overrides this and forces a Chat Room. Keep this logic in `src-tauri/src/matrix/rooms.rs`, with unit tests.
- E2EE is required in v1: recovery key restore, SAS emoji verification, and a visible "unable to decrypt" placeholder that retries.
- Presence may be disabled on the server. Never assume it exists; fall back to sorting by recent activity.

## Constraints
- Recreate the AIM *style* only. Never add AOL logos, the running-man mascot, original AOL sound files or other copied assets. All art and sounds must be original.
- Never log access tokens, recovery keys or message plaintext.
- Store the session in the OS keyring, not in plain files.

## Commands
- Dev: `cargo tauri dev`
- Rust tests: `cargo test --manifest-path src-tauri/Cargo.toml`
- Local test homeserver (see `dev/README.md`): `docker compose -f dev/docker-compose.yml up -d`, then `python3 dev/seed.py`. Users `alice`/`bob`/`carol`, password `aimtrix-test`, homeserver `http://localhost:8008`, Element Web at `http://localhost:8080`. Always test against this, never real accounts.
