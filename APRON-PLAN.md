# AIMtrix on Apron: a second network

*Tabled. Not scheduled; nothing here starts until asked. Written 2026-11-08 against Apron protocol v10.*

## Context
[Apron](https://apron.chat/protocol) is a small, open chat protocol: JSON frames over one WebSocket, an authoritative server, no federation and no end-to-end encryption (TLS only, by design). It's aimed at small groups that trust their server. This plan adds it to AIMtrix as a second network beside Matrix, so the same AIM-style Buddy List, IM and Chat Room windows work on an Apron server.

- Start here: [apron.chat/llms.txt](https://apron.chat/llms.txt) indexes everything below (the spec in parts, servers, clients, schema, fixtures); [llms-full.txt](https://apron.chat/llms-full.txt) is the whole spec in one file.
- Spec: [`shazow/apron`](https://github.com/shazow/apron): `PROTOCOL.md` (authoritative), `RECIPES.md`, a JSON Schema (`schema/apron.schema.json`) and conformance fixtures (`tests/fixtures`). CC0 and MIT.
- Servers: [`apron-chat/apron-server-go`](https://github.com/apron-chat/apron-server-go) (`aprond`, the reference server: one Go binary with SQLite and every v10 capability), [`apron-server-cloudflare`](https://github.com/apron-chat/apron-server-cloudflare) (the public demo at `wss://server.apron.chat/`, limited), and minimal Python and JS servers in `shazow/apron/servers`. All MIT.
- Clients to compare against: [`apron-web`](https://github.com/apron-chat/apron-web) (SvelteKit, live at web.apron.chat).

## Verdict: feasible, about a phase of work
The protocol is small, so the Apron client itself is the easy part. Most of the work is in AIMtrix: its Rust core is built on matrix-sdk throughout, so a second network first needs a shared interface.

| AIM | Matrix (today) | Apron |
|---|---|---|
| Sign On | Password or browser (OAuth) | `auth` with `token`, `email` (code), `external` (web sign-in) or `guest`; `webauthn` (passkeys) later |
| Server | Homeserver URL | WebSocket URL, such as `wss://chat.example/ws`; no discovery |
| Screen name | Display name, MXID | `name`, server-assigned `user_id` (the server decides who everyone is) |
| Buddy (IM) | DM room | A private room (`private: true`) with exactly two members (DMs are just private rooms, per Apron's author), the other added with `room_join` and a `user_id` (`aprond` allows this; other servers MAY) |
| Chat Room | Any other room | Any other room or thread (`rooms` capability) |
| Buddy List | Joined rooms | `room_list` with `members: true` |
| Online/Away/Offline | Presence, if the server has it | `status` capability: `online`, `idle`, `dnd`, `invisible`, `offline` |
| Away message | `status_msg` | Status text if the server keeps one; otherwise auto-reply only |
| Typing | `m.typing` | `activity` capability (`typing`, `read_message_id`) |
| Rich text | `formatted_body` HTML | `body.format: "markdown"` (CommonMark); AIMtrix's font colors don't carry over |
| Edits, replies, reactions | Phase 11 | `edit`, `reply_to`, `reactions` |
| Send File | Media repo | `embed:upload`: the server returns a `write_url`, the client PUTs the file |
| History | Sliding sync, back-pagination | `history`: page an append-only log by `log_id` |
| Encryption | Required (recovery, verification) | None; the server reads everything |

## Decisions to make before starting
- **Encryption.** `CLAUDE.md` requires E2EE. Apron has none by design. Either accept that (the rule becomes "on Matrix"; Sign On says plainly that an Apron server can read your messages; recovery, verification and "unable to decrypt" hide on Apron accounts) or don't do this plan.
- **One network at a time, or both at once.** Picking Matrix or Apron at Sign On fits the current one-session core. Being signed on to both, as old multi-network clients did, means several sessions, a merged Buddy List and per-account windows: much bigger. Start with one at a time.
- **What a buddy is.** Apron's author confirmed DMs are just private rooms, so AIMtrix treats a two-member private room as an IM, and other Apron clients see the same room.
- **Where it goes in the plan.** After Phase 12 (Sounds), or earlier if wanted. The shared interface (A.1) is worth doing on its own if any other network is ever likely.

## Steps
- [ ] **A.1 A network interface in the core.** Put what the windows need from a chat account (sign on/off, resume, the room list, timelines, sending, members, typing, presence, uploads) behind a Rust trait, with matrix-sdk as its first implementation. `commands.rs`, `session.rs` and the event names stay as they are; Matrix-only parts (recovery, verification) are optional parts of the trait. No visible change. *Done when:* every existing unit and server test passes on every dev server, unchanged.
- [ ] **A.2 Dev Apron servers.** Add an `apron` profile to `dev/docker-compose.yml` running `aprond` (pinned version, SQLite volume, email codes written to its log), seeded with `alice`/`bob`/`carol` and a couple of rooms, plus the minimal Python server as a second implementation. `AIMTRIX_APRON` points the tests at either, like `AIMTRIX_HS`. *Done when:* `apron-web` signs on to both and the seed users can chat.
- [ ] **A.3 Apron client in the core.** A WebSocket client (`tokio-tungstenite`, MIT): `auth`, the `server` and `user` notifications, `message`, capability detection, reconnecting and catching up with `history` (buffer live records, page up to `latest_log_id`, keep the greatest `log_id` per key). Tokens go in the OS keyring like Matrix sessions. Unit tests run the conformance fixtures from `shazow/apron/tests/fixtures`. *Done when:* the fixtures pass and a server test signs on, sends and receives on `aprond`.
- [ ] **A.4 Sign On and the Buddy List on Apron.** Sign On gets a network choice (Matrix or Apron) and an Apron server address; token and email sign-in first, `external` through the browser like Use Browser…. The Buddy List shows two-member private rooms as Buddies and other rooms as Chat Rooms; Add Buddy creates the private room and adds the other user. Status from the `status` capability where the server has it, else sort by recent activity as on Matrix. Sign On warns that the server can read your messages. *Done when:* alice signs on to `aprond`, sees bob, and opens an IM.
- [ ] **A.5 IMs and Chat Rooms on Apron.** Timelines with history paging, sending (markdown), typing, read markers, edits, replies, reactions (when Phase 11 exists), Send File through `embed:upload`, member lists, join and leave notices. Embeds AIMtrix can't show (`iframe`, `html`, `terminal`) appear as a link or a placeholder, never as live HTML. *Done when:* a conversation between AIMtrix and `apron-web` works both ways, including files.
- [ ] **A.6 Other servers.** Run the Apron server tests against the minimal server and the public demo (by hand), and log where servers differ. *Done when:* the findings are written down here and gaps are handled or logged.

## Risks
- **A young protocol.** v10, changing weekly. Pin the protocol version AIMtrix targets, read `HISTORY.md` before each step, and lean on the fixtures to notice changes.
- **The refactor touches working code.** A.1 must land with no behaviour change, checked by the full server test suite, before any Apron code.
- **Server differences.** Private rooms, adding other users and status are optional capabilities; a server without them gives a reduced Buddy List. Show what's missing instead of failing.
- **Untrusted content.** Embeds include raw HTML and iframes. Render nothing active; sanitize markdown as the spec requires.

## Known issues and follow-ups
- None yet.
