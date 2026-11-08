# Development and testing

## Prerequisites

- **Rust** (stable, via rustup) and **Node 22**. `mise.toml` defines the project tasks; `mise tasks` lists them.
- **Linux build libraries** (Debian/Ubuntu names): `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev libxdo-dev libayatana-appindicator3-dev`. The CI workflow installs exactly these.
- **Docker** with Compose, for the dev homeservers.
- **Python 3**, for `dev/seed.py` and the other dev scripts.
- A running **keyring** (Secret Service on Linux: GNOME Keyring or KWallet; Keychain on macOS), since the session is stored there.

## Running

```sh
npm install          # also installs the Tauri CLI (no global cargo-tauri needed)
npm run tauri dev    # or: mise run dev
```

`tauri dev` starts Vite on `http://localhost:1420` (`beforeDevCommand` in `tauri.conf.json`) and runs the Rust app pointed at it, with hot reload for the frontend and a rebuild on Rust changes. Dev builds list the local dev homeservers in Sign On and link to `/demo` from the Buddy List.

`/demo` also opens in a normal browser at `http://localhost:1420/demo` while `npm run dev` runs, since it makes no Tauri calls. The other routes need Tauri.

Logs (`eprintln!`) go to the terminal running `tauri dev`. They never contain tokens, keys or message text.

## Dev homeservers

Test against these, never real accounts. Full details are in [`dev/README.md`](../dev/README.md).

```sh
docker compose -f dev/docker-compose.yml up -d     # Synapse + Element Web
python3 dev/seed.py                                # users and rooms (safe to re-run)
docker compose -f dev/docker-compose.yml down -v   # stop and wipe
```

| Profile | Services | Homeserver | Use |
|---|---|---|---|
| (default) | `synapse`, `element` | `http://localhost:8008`; Element Web on `:8080` | The main dev server, with presence. |
| `no-presence` | `synapse-no-presence` | `http://localhost:8009` | Like matrix.org: no presence. |
| `continuwuity` | `continuwuity` | `http://localhost:8011` | A different server implementation; registration token instead of an admin API; its own OAuth. |
| `mas` | `synapse-mas`, `mas-db`, `mas`, `dex`, `mas-proxy` | `http://localhost:8013`; MAS on `:8014`; Dex on `:5556` | Synapse behind Matrix Authentication Service, like matrix.org. Dex plays "Dev Provider (like Google)". |

Seed each with `AIMTRIX_HS=<homeserver> python3 dev/seed.py`. Users are `alice`, `bob` and `carol` with password `aimtrix-test`; on MAS also `dana`, browser sign-on only. Seeded rooms (DMs, an encrypted DM, a group with scrollback, an encrypted group, an invitation) are listed in `dev/README.md`.

Other dev tools:
- `python3 dev/old-server.py`: a fake homeserver on `:8010` without sliding sync, which AIMtrix should refuse.
- `python3 dev/clean-devices.py`: signs out leftover test devices, which slow encryption down (not on MAS).

## Tests

```sh
npm run check                                              # frontend type-check (svelte-check)
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings   # lint; CI fails on any warning
cargo test --manifest-path src-tauri/Cargo.toml            # Rust unit tests: no server needed
mise run check                                             # all three
```

**Unit tests** sit next to the code (`#[cfg(test)] mod tests`). They cover the pure rules: room classification (`rooms.rs`), timeline line mapping (`timeline.rs`), HTML sanitizing (`html.rs`), auto-reply decisions (`auto_reply.rs`), keyring formats (`keyring.rs`), preferences validation, tray icon drawing, window labels, routes and the navigation guard, the OAuth page check, file names for saving, upload preparation (including turned photos), and the sign-on refusal against a fake server.

**Server tests** are `#[ignore = "needs the dev homeserver"]` and run against the dev servers:

```sh
docker compose -f dev/docker-compose.yml --profile no-presence --profile continuwuity --profile mas up -d
python3 dev/seed.py
AIMTRIX_HS=http://localhost:8009 python3 dev/seed.py
AIMTRIX_HS=http://localhost:8011 python3 dev/seed.py
AIMTRIX_HS=http://localhost:8013 python3 dev/seed.py

cargo test --manifest-path src-tauri/Cargo.toml -- --include-ignored --test-threads=8   # mise run test-server
```

- Every profile must be up and seeded: some tests always use a particular server (presence on or off, MAS, continuwuity).
- `AIMTRIX_HS` picks the server for the general tests (default `http://localhost:8008`), the same variable `dev/seed.py` reads. To run the whole suite against continuwuity: `AIMTRIX_HS=http://localhost:8011 cargo test … -- --include-ignored`.
- Keep `--test-threads=8`; more overloads the dev servers and tests time out.
- Most server tests are at the bottom of `src-tauri/src/matrix/mod.rs`. They sign on as real users or throwaway users (`register_throwaway_user`, which uses Synapse's admin API, a registration token or MAS's admin API as the server allows), and they **sign off at the end** so devices don't pile up. Browser sign-on tests drive MAS, Dex and continuwuity's sign-in pages over HTTP (`sign_on_in_browser`).
- None of them touch the OS keyring.
- Killed or failed runs leave devices behind; run `python3 dev/clean-devices.py` when tests get slow.

## CI

`.github/workflows/ci.yml` runs on every push to `main` (merged pull requests), not on pull requests themselves, because a full run takes a while.

- **Build** on `ubuntu-22.04`, `macos-latest` and `windows-latest`: `npm ci`, `npm run check`, clippy (any warning fails), the Rust unit tests, and a release build without bundling (`tauri build --no-bundle`, with LTO off to save time). Linux also checks the third-party licenses are current, and frees disk space first.
- **Server tests** on Linux: starts every dev profile in Docker, seeds them, and runs the full suite with `--include-ignored --test-threads=8`, printing server logs on failure.

Debug info is off in CI (`CARGO_PROFILE_DEV_DEBUG=0`), and a newer push cancels an older run.

## Licenses

AIMtrix is MIT. Dependencies must allow that: `src-tauri/about.toml` lists the accepted licenses, and `cargo about` refuses others. After changing Rust or npm dependencies:

```sh
mise run licenses    # node dev/third-party-licenses.mjs; needs: cargo install --locked cargo-about@0.9.3 --features cli
```

This regenerates `src-tauri/THIRD-PARTY-LICENSES.html`, which ships with every package and opens from About AIMtrix. CI fails if it's stale (`--check`). `dev/appimage-notices.sh` adds the licenses of the system libraries bundled into the AppImage.

## Versions

Until AIMtrix goes public, builds are numbered `0.0.1-<seconds since midnight UTC>` (`dev/set-version.mjs`). The repo stays at the base version, `0.0.1`; the build version is only written into build copies (the package container, the release workflow, or `--config` for the `.dmg`).

## Packages

| Task | Script | Output |
|---|---|---|
| `mise run package` | `dev/build-package.sh` | `.deb` and `.AppImage` in `dist/`, built in an Ubuntu 22.04 container so they run on Ubuntu 22.04, Debian 12 and newer |
| `mise run test-package [image]` | `dev/test-package.sh` | Installs the newest `.deb` on a clean container (default `ubuntu:24.04`), signs on as a throwaway user and checks the session resumes; screenshots in `dev/package-test/` |
| `mise run package-mac` | `dev/build-dmg.sh` | On a Mac: a universal (Intel and Apple Silicon), unsigned `.dmg` in `dist/` |

## Releases

`mise run release` (`dev/release.sh`) refuses unless you're on `main`, with no uncommitted changes and HEAD equal to `origin/main` (CI checks only `main`), then tags HEAD `v<version>` and pushes the tag. `.github/workflows/release.yml` then:
1. **`release`**: checks the tag is a valid version on a commit in `main`, and creates a draft GitHub release with notes (re-runs keep the existing draft).
2. **`linux`**: builds the `.deb` and `.AppImage` on Ubuntu 22.04 with `--locked` (exactly the crates the notices list), adds the bundled-library notices, and attaches them (`--clobber`, so re-runs replace them).
3. **`macos`**: builds the `.dmg` and attaches it.

You review the draft on GitHub and publish it.

## Project docs

| File | What |
|---|---|
| `README.md` | What AIMtrix is and does, for users |
| `PLAN.md` | The design, the AIM→Matrix mapping, the step-by-step build plan, known issues |
| `CHANGELOG.md` | What each finished step changed |
| `CLAUDE.md` | Working rules, architecture rules, constraints, commands |
| `dev/README.md` | The dev homeservers in detail |
| `docs/` | These developer docs |
