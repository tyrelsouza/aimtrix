# Local Matrix test environment

A throwaway Synapse homeserver plus Element Web, so AIMtrix can be tested without real accounts.

```sh
docker compose -f dev/docker-compose.yml up -d   # start
python3 dev/seed.py                              # create users and rooms (safe to re-run)
docker compose -f dev/docker-compose.yml down -v # stop and wipe all data
```

A second Synapse with presence turned off acts like matrix.org and other servers that do the same. The Rust tests need it too:

```sh
docker compose -f dev/docker-compose.yml --profile no-presence up -d
AIMTRIX_HS=http://localhost:8009 python3 dev/seed.py
```

continuwuity, a different homeserver implementation (Rust, from conduwuit), for checking AIMtrix isn't tied to Synapse:

```sh
docker compose -f dev/docker-compose.yml --profile continuwuity up -d
AIMTRIX_HS=http://localhost:8011 python3 dev/seed.py
AIMTRIX_HS=http://localhost:8011 cargo test --manifest-path src-tauri/Cargo.toml -- --include-ignored --test-threads=8
```

It has no shared-secret admin API, so the seed script and the tests register users with the registration token `aimtrix-dev-token` instead. Its first account is the server admin; the container creates `admin` (password `aimtrix-test`) at startup so that isn't alice. Known differences are under "Known issues" in `PLAN.md`.

Synapse behind Matrix Authentication Service (MAS), set up like matrix.org: Synapse delegates accounts and sign-on to MAS, and password sign-on still works through MAS's compatibility layer:

```sh
docker compose -f dev/docker-compose.yml --profile mas up -d
AIMTRIX_HS=http://localhost:8013 python3 dev/seed.py
AIMTRIX_HS=http://localhost:8013 cargo test --manifest-path src-tauri/Cargo.toml -- --include-ignored --test-threads=8
```

An nginx container (`mas/nginx.conf`) is the homeserver on 8013: it sends `/login`, `/logout` and `/refresh` to MAS and everything else to Synapse. MAS itself (its account pages, and OAuth for 7.4) is on 8014. Accounts are made with MAS's admin API, using the admin client in `mas/config.yaml`. As on matrix.org, resetting your encryption identity needs approval in MAS's web page instead of a password; the tests approve it the way MAS does (`approve_identity_reset`). `clean-devices.py` doesn't work here: MAS doesn't allow deleting devices with a password.

MAS also offers "Dev Provider (like Google)", a [Dex](https://dexidp.io) container (`mas/dex.yaml`, on 5556) standing in for Google, GitHub or Apple. Choosing it on MAS's sign-in page (not the username and password fields there) and signing in to Dex as `dana@example.com` / `aimtrix-test` makes (or reuses) the account `@dana:localhost`, which has no password: it can only sign on with **Use Browser…**, like a matrix.org account made with Google. Access tokens here last 60 seconds instead of 5 minutes, so token refresh happens while you test.

Browser sign-on also works on continuwuity, which has its own OAuth (no MAS); `CONTINUWUITY_WELL_KNOWN__CLIENT` makes it advertise `localhost:8011` for it.

| What | Where |
|---|---|
| Homeserver | `http://localhost:8008` (server name `localhost`, presence on) |
| Homeserver without presence | `http://localhost:8009` (same users and rooms once seeded; `synapse/no-presence.yaml` overrides `homeserver.yaml`) |
| continuwuity | `http://localhost:8011` (server name `localhost`, presence on; same users and rooms once seeded) |
| Synapse behind MAS | `http://localhost:8013` (homeserver, server name `localhost`); `http://localhost:8014` (MAS: account pages, OAuth); `http://localhost:5556` (Dex, the "Dev Provider") |
| Fake old homeserver | `http://localhost:8010`, from `python3 dev/old-server.py`: no simplified sliding sync, so AIMtrix should refuse to sign on |
| Element Web | `http://localhost:8080`, the "other person" to chat with AIMtrix |
| Users | `alice`, `bob`, `carol`, password `aimtrix-test`; on the MAS server also `dana`, browser only (see above) |

## Seeded rooms (from Alice's point of view)

| Room | Type | Encrypted | Notes |
|---|---|---|---|
| Bob Brown | DM with bob | no | A few messages, one with HTML formatting |
| Carol Chen | DM with carol | yes | Empty; send messages from Element |
| Test Chat | Group (all 3) | no | ~60 messages for scrollback, room avatar, favorited by Alice |
| Secret Chat | Group (all 3) | yes | Empty |
| Invite Test | Group | no | Carol invited Alice; Alice hasn't joined |

Alice and Bob have solid-color avatars. Carol has none, to test the default Buddy Icon.

The seed script can't send encrypted messages (it has no crypto). To get encrypted history, send messages in those rooms from Element Web.

**Leftover devices:** tests that fail or are killed leave "AIMtrix" devices on the test accounts, which slows encryption down. `python3 dev/clean-devices.py` signs out the ones not seen for an hour (`--dry-run` to just count; `AIMTRIX_HS=…` for another server).

**Recovery (key backup):** the Rust tests give bob recovery with the passphrase `bob test recovery passphrase`, resetting his cross-signing identity the first time. Leave bob's recovery to the tests. To try Restore Messages by hand, set up recovery for alice in Element (Settings → Encryption → Set up recovery) and keep the key it shows.

**Dev only:** the secrets in `synapse/homeserver.yaml` are public and rate limits are effectively off. Ports are bound to `127.0.0.1`.
