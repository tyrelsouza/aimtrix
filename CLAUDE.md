# AIMtrix

A Matrix client that looks and behaves like AOL Instant Messenger. See `PLAN.md` for the full design, the AIM→Matrix mapping and the build phases.

## Workflow
- Work through the `## Steps` checklist in `PLAN.md` one step at a time, in order. Don't start the next step unless asked.
- A step is finished only when its "Done when" check passes. Then tick its box (`[x]`) in `PLAN.md`.
- Every time you tick a step, add an entry for it to the "Changes" list of its phase in `CHANGELOG.md`, starting a new phase section if needed. Say what changed for the user first, then any notable internals and fixes. Keep "Known issues" in `PLAN.md`, not in the changelog.
- If a step turns out bigger than expected, split it into sub-steps in `PLAN.md` instead of doing it all at once.
- Record problems found along the way that don't belong to a step under "Known issues and follow-ups" in `PLAN.md`.
- Never commit. The user makes all commits themselves, outside Claude. Don't run `git commit` or other history-changing git commands, don't offer to commit, and don't comment on commit status.

## Stack
- **Tauri v2** desktop app.
- **Rust core** (`src-tauri/`): runs a single `matrix-sdk` (matrix-rust-sdk) `Client` with an SQLite store and E2EE. All Matrix logic lives here.
- **Frontend** (`src/`): SvelteKit (Svelte 5, TypeScript, `adapter-static` SPA mode, `ssr = false`) and hand-written Win98/XP-style CSS. One route per window type. Windows are thin views with no Matrix SDK in JS.
- **Styling:** reuse the `aim-` classes and CSS tokens in `src/styles/aim-chrome.css` (loaded globally from `src/routes/+layout.svelte`) instead of one-off styles. `/demo` (dev-only link on the main page) shows every piece; add new pieces there too.

## Architecture rules
- One Matrix session, owned by the Rust core. Windows talk to it only through `#[tauri::command]`s (`src-tauri/src/commands.rs`) and Tauri events (`sync:status`, `buddylist:update`, `room:<id>:timeline_diff`, `room:<id>:typing`, `room:<id>:members`, `room:<id>:topic`, `room:<id>:upload`, `recovery:status`, `presence:update`, `preferences:update`, `verification:*`). In event names `<id>` is the room's key (`windows::room_key`, the hash in its window label), since room IDs have characters event names can't.
- Windows are created in code (`src-tauri/src/windows.rs`), not in `tauri.conf.json`. Add every new window label to `src-tauri/capabilities/default.json`, or it can't listen to events or close itself (commands still work, so it's easy to miss). On launch `startup()` in `src-tauri/src/lib.rs` resumes the saved session into the Buddy List or opens Sign On.
- The Buddy List is the main window. Each IM or Chat Room gets its own `WebviewWindow`, labeled `room-<roomId-hash>`. Opening a room that already has a window focuses that window.
- DM vs. group: a room is a DM if it is in `m.direct` OR has ≤2 joined+invited members. The `aimtrix.chat` room tag overrides this and forces a Chat Room. Keep this logic in `src-tauri/src/matrix/rooms.rs`, with unit tests.
- E2EE is required in v1: recovery key restore, SAS emoji verification, and a visible "unable to decrypt" placeholder that retries.
- Presence may be disabled on the server. Never assume it exists; fall back to sorting by recent activity.

## Constraints
- Recreate the AIM *style* only. Never add AOL logos, the running-man mascot, original AOL sound files or other copied assets. All art and sounds must be original.
- Never log access tokens, recovery keys or message plaintext.
- Store the session in the OS keyring, not in plain files.

## Commands
- Install: `npm install` (the Tauri CLI is a dev dependency; no global `cargo tauri`)
- Dev: `npm run tauri dev`
- Type-check frontend: `npm run check`
- Rust tests: `cargo test --manifest-path src-tauri/Cargo.toml`. Tests that need the dev homeserver are `#[ignore = "needs the dev homeserver"]`; run them with `-- --include-ignored --test-threads=8` (every dev server up and seeded: the default, `no-presence`, `continuwuity` and `mas` profiles, since some tests always use a particular one; more threads overload the dev server and tests time out). They must log out so no devices pile up on the test accounts; failed or killed runs still leave some, so run `python3 dev/clean-devices.py` when tests get slow.
- Local test homeserver (see `dev/README.md`): `docker compose -f dev/docker-compose.yml up -d`, then `python3 dev/seed.py`. A second one with presence off runs on `http://localhost:8009`: add `--profile no-presence` to `up`, and seed it with `AIMTRIX_HS=http://localhost:8009 python3 dev/seed.py`. Users `alice`/`bob`/`carol`, password `aimtrix-test`, homeserver `http://localhost:8008`, Element Web at `http://localhost:8080`. Always test against this, never real accounts. A continuwuity server (a different implementation) runs on `http://localhost:8011` with `--profile continuwuity`; seed it and run the server tests against it with `AIMTRIX_HS=http://localhost:8011` (the tests read the same variable as `dev/seed.py`). Likewise Synapse behind Matrix Authentication Service, like matrix.org: `--profile mas`, homeserver `http://localhost:8013`, MAS on `http://localhost:8014`.
- Tasks: `mise tasks` lists shortcuts for all of these (`mise run check`, `mise run test-server`, …).
- Packages: `mise run package` (`dev/build-package.sh`) builds the `.deb` and `.AppImage` in an Ubuntu 22.04 container into `dist/`, so they run on Ubuntu 22.04/Debian 12 and newer (a host build on a rolling distro needs that distro's glibc). `mise run test-package [image]` installs the `.deb` on a clean container, signs on as a throwaway user and checks the session resumes, leaving screenshots in `dev/package-test/`.
- Versions: until AIMtrix goes public, builds are `0.0.1-<seconds since midnight UTC>` (`dev/set-version.mjs`). The repo stays at the base version; the version is written only into build copies.
- CI: `.github/workflows/ci.yml` runs on pull requests and pushes to `main`: `npm run check`, the Rust unit tests and a release build, and the server tests against every dev server in Docker, on Linux. macOS and Windows build only on pushes to `main` (private repo; their minutes cost more). Keep code building on all three.
- Releases: `mise run release` tags HEAD and pushes the tag; `.github/workflows/release.yml` builds on Ubuntu 22.04 and attaches the packages to a draft GitHub release. Only the user runs it.
