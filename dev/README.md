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

| What | Where |
|---|---|
| Homeserver | `http://localhost:8008` (server name `localhost`, presence on) |
| Homeserver without presence | `http://localhost:8009` (same users and rooms once seeded; `synapse/no-presence.yaml` overrides `homeserver.yaml`) |
| Fake old homeserver | `http://localhost:8010`, from `python3 dev/old-server.py`: no simplified sliding sync, so AIMtrix should refuse to sign on |
| Element Web | `http://localhost:8080`, the "other person" to chat with AIMtrix |
| Users | `alice`, `bob`, `carol`, password `aimtrix-test` |

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

**Leftover devices:** tests that fail or are killed leave "AIMtrix" devices on the test accounts, which slows encryption down. `python3 dev/clean-devices.py` signs out the ones not seen for an hour (`--dry-run` to just count).

**Recovery (key backup):** the Rust tests give bob recovery with the passphrase `bob test recovery passphrase`, resetting his cross-signing identity the first time. Leave bob's recovery to the tests. To try Restore Messages by hand, set up recovery for alice in Element (Settings → Encryption → Set up recovery) and keep the key it shows.

**Dev only:** the secrets in `synapse/homeserver.yaml` are public and rate limits are effectively off. Ports are bound to `127.0.0.1`.
