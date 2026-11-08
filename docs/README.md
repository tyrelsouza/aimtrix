# AIMtrix developer docs

How AIMtrix is built: what runs where, where things start, and how the pieces talk. For what AIMtrix does, see the top-level [`README.md`](../README.md); for what's planned, [`PLAN.md`](../PLAN.md); for the rules of working on it, [`CLAUDE.md`](../CLAUDE.md).

## Reading order

1. **[Architecture](architecture.md):** the big picture. One Rust process owns the only Matrix session, many thin webview windows show it, and commands and events connect them. Includes the main flows end to end: launch, sign-on, opening an IM, sending a message, signing off.
2. **[Backend](backend.md):** the Rust core in `src-tauri/`, file by file: entry points, the app-wide state, the session lifecycle, every `matrix/` module, windows, tray and preferences, and how to add a command, event or window.
3. **[Frontend](frontend.md):** the SvelteKit app in `src/`: one route per window type, the shared layout and title bar, the `$lib` modules that wrap commands and events, the conversation components, and the AIM chrome CSS.
4. **[IPC reference](ipc.md):** every command and event, with arguments, payloads and who uses them.
5. **[Development and testing](development.md):** running it, the dev homeservers, the test suites, CI, packages and releases.

Past reviews are in [`reviews/`](reviews/): [2026-11-08 bug and security review](reviews/2026-11-08-bug-review.md).

## The shortest possible tour

```
src-tauri/src/main.rs        process entry: sets a Linux env var, calls aimtrix_lib::run()
src-tauri/src/lib.rs         run(): plugins, app-wide state, the command list; startup() resumes or shows Sign On
src-tauri/src/commands.rs    every #[tauri::command] the windows can call
src-tauri/src/session.rs     the signed-on session's lifecycle and the events it sends to windows
src-tauri/src/windows.rs     creating each window (labels, sizes, routes)
src-tauri/src/matrix/        all Matrix logic (matrix-sdk): sign-on, sync, rooms, timelines, crypto…

src/routes/+layout.svelte    wraps every window in the AIM frame and title bar
src/routes/<window>/         one route per window type: signon, / (Buddy List), im, chat, away…
src/lib/*.ts                 typed wrappers around commands and events, mirroring the Rust types
src/styles/aim-chrome.css    the Win98/XP look; /demo shows every piece
```

If you only read three files, read `lib.rs`, `session.rs` and `src/routes/+page.svelte` (the Buddy List).

## Glossary

| Term | Meaning |
|---|---|
| Buddy List | The main window (label `main`, route `/`): your rooms in groups. |
| IM | The window for a DM (route `/im`). |
| Chat Room | The window for a room with several people (route `/chat`). |
| Buddy | One entry on the Buddy List: a room, shaped by `rooms::Buddy`. |
| Room key | A 16-hex-digit FNV-1a hash of a room ID (`windows::room_key`). Room windows are labeled `room-<key>` and their events are `room:<key>:…`, because room IDs contain characters labels and event names can't. |
| Store | The encrypted SQLite store matrix-sdk keeps in the app data folder, one folder per sign-on (`stores/<name>/`): room state, crypto keys, media cache. |
| Saved session | What's needed to resume without signing on again, kept in the OS keyring (`matrix/keyring.rs`). |
| Sliding sync | Simplified sliding sync (MSC4186), the only sync AIMtrix uses. Servers without it are refused at sign-on. |
| UTD | "Unable to decrypt": an encrypted message whose key hasn't arrived. Shown as a placeholder that turns into the message by itself. |
