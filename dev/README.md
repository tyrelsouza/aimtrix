# Local Matrix test environment

A throwaway Synapse homeserver plus Element Web, so AIMtrix can be tested without real accounts.

```sh
docker compose -f dev/docker-compose.yml up -d   # start
python3 dev/seed.py                              # create users and rooms (safe to re-run)
docker compose -f dev/docker-compose.yml down -v # stop and wipe all data
```

| What | Where |
|---|---|
| Homeserver | `http://localhost:8008` (server name `localhost`, presence on) |
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

**Dev only:** the secrets in `synapse/homeserver.yaml` are public and rate limits are effectively off. Ports are bound to `127.0.0.1`.
