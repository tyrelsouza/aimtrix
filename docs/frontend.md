# Frontend: the windows

Everything in `src/`. It's a SvelteKit app (Svelte 5 with runes, TypeScript) built by `@sveltejs/adapter-static` as a single-page app (`ssr = false` in `src/routes/+layout.ts`, `fallback: "index.html"` in `svelte.config.js`). Every Tauri window loads the same app at a different route, so **each route is one window type**.

Windows hold no Matrix state and import no Matrix SDK. They only `invoke` commands and `listen` for events from `@tauri-apps/api`. All logic that can be tested (is this a DM, which group, is this HTML safe) lives in Rust; the frontend renders and handles input.

```
src/
  app.html                     the HTML shell
  routes/
    +layout.ts                 ssr = false
    +layout.svelte             AIM frame and title bar around every window; loads aim-chrome.css
    +page.svelte               Buddy List                  (window "main")
    signon/+page.svelte        Sign On                     ("signon")
    im/+page.svelte            IM window                   ("room-<key>", ?room=<room ID>)
    chat/+page.svelte          Chat Room window            ("room-<key>", ?room=<room ID>)
    away/+page.svelte          Away Message                ("away")
    add-buddy/+page.svelte     Add Buddy                   ("add-buddy")
    preferences/+page.svelte   Preferences                 ("preferences")
    about/+page.svelte         About AIMtrix               ("about")
    recovery/+page.svelte      Restore Messages            ("recovery")
    setup-recovery/+page.svelte Set Up Recovery            ("setup-recovery")
    verify/+page.svelte        Verify This Computer        ("verify")
    demo/+page.svelte          dev-only style showcase (no window; open in a browser)
  lib/
    TitleBar.svelte            the AIM-style title bar
    conversation/
      Transcript.svelte        the message list (IM and Chat Room)
      Compose.svelte           the toolbar, message box and Send button
      status.ts                the conversation status bar text
      upload.ts                Send File: drops, pastes, progress
    watch.ts                   watchValue: follow a value Rust keeps (listen, then ask)
    sync.ts buddylist.ts timeline.ts chatroom.ts presence.ts recovery.ts verify.ts preferences.ts
                               typed command and event wrappers, one per area
    compose.ts                 editor → Matrix HTML; formatting in received messages
    icons.ts                   default Buddy Icons (original SVG)
    platform.ts                isMac, shortcutKey ("Cmd"/"Ctrl")
  styles/aim-chrome.css        the Win98/XP look
```

## Entry points

There's no single `main.ts`. Each window's entry is:
1. **Rust** creates the window at a route (`windows.rs`), e.g. `/im?room=%21abc%3Alocalhost`.
2. **`+layout.svelte`** runs first in every window. It imports `aim-chrome.css` and wraps the page:
   ```svelte
   <div class="aim-window aim-frame">
     <TitleBar />
     <div class="aim-frame__content">{@render children()}</div>
   </div>
   ```
3. **The route's `+page.svelte`** reads its parameters (`page.url.searchParams.get("room")`), starts its watches in `$effect`s, and renders.

Which window opens first is decided in Rust (`startup()` in `lib.rs`): the Buddy List if a saved session resumes, otherwise Sign On.

## `TitleBar.svelte`

Windows have no system decorations (`decorations(false)`), so every window draws this: the app icon, the window title (from `getCurrentWindow().title()`, set in Rust), and minimize, maximize (only on resizable windows) and close buttons. Mouse-down on the bar calls `startDragging()`; a double-click toggles maximize. It greys out when the window loses focus (`onFocusChanged`). The permissions it needs (`allow-start-dragging`, `allow-minimize`, `allow-toggle-maximize`, `allow-close`) are in `src-tauri/capabilities/default.json`.

## The watch pattern

Almost every `$lib` module has a `watch…(onChange)` function. Those that follow one whole value use `watchValue` from `$lib/watch.ts`:

```ts
export function watchBuddyList(onChange: (buddies: Buddy[]) => void): () => void {
  return watchValue("buddylist:update", "buddy_list", onChange);
}
```

`watchValue(event, command, onChange)`:
1. Subscribes to the event, and waits until the subscription is in place.
2. Then asks for the current value with the command; Rust keeps the latest value of every such event for this. If an event arrived meanwhile, the answer is dropped, since the event is newer.
3. Returns a stop function, which is exactly what a Svelte `$effect` cleanup wants:
   ```ts
   $effect(() => watchBuddyList((b) => (buddies = b)));
   ```

`watchChatRoom` does the same for two events at once. `watchTimeline` is the exception: its events are diffs relative to the lines, so diffs that arrive before the first `room_timeline` answer are queued and applied on top of it. Each answer and diff carries a watch number, and only diffs with the answer's number are applied: after a reload, diffs from the watch it replaced are ignored.

| Module | Watch function | Command, then event |
|---|---|---|
| `sync.ts` | `watchSyncStatus` | `sync_status`, `sync:status` |
| `buddylist.ts` | `watchBuddyList` | `buddy_list`, `buddylist:update` |
| `presence.ts` | `watchOwnPresence` | `own_presence`, `presence:update` |
| `recovery.ts` | `watchRecovery` | `recovery_status`, `recovery:status` |
| `verify.ts` | `watchTrust`, `watchStep` | `device_trust`, `verification:trust`; `verification_step`, `verification:step` |
| `preferences.ts` | `watchPreferences` | `get_preferences`, `preferences:update` |
| `timeline.ts` | `watchTimeline`, `watchTyping` | `room_timeline`, `room:<key>:timeline_diff`; `room:<key>:typing` |
| `chatroom.ts` | `watchChatRoom` | `chat_room`, `room:<key>:members` and `room:<key>:topic` |
| `conversation/upload.ts` | `watchUpload`, `watchFileDrops` | `room:<key>:upload`; the webview's drag-and-drop event |

New watches of one whole value should use `watchValue` rather than repeat the listen-then-ask steps.

Room windows find their key with `roomKey()` in `timeline.ts`: the window's own label minus `room-`. They never compute the hash themselves.

## Types mirror Rust

Every payload has a TypeScript type with a comment naming its Rust original, e.g. `/** Mirrors \`Buddy\` in src-tauri/src/matrix/rooms.rs. */`. Serde decides the shapes:
- Structs are camelCase (`roomId`, `awayMessage`).
- Enum values are kebab-case strings (`"before-sign-on"`, `"no-cross-signing"`), except `Diff`'s `op`, which is camelCase (`pushFront`).
- Tagged unions use a tag field: `Line` by `type`, `Diff` by `op`, `Step` by `step`, `Upload` and `SyncStatus` by `state`.

When you change a Rust type that crosses IPC, change its mirror too. Nothing checks them against each other automatically.

The same modules hold the presentation helpers for those types, kept next to them: `describe` (status bar text), `describeUndecryptable`, `describeMemberChange`, `describeTyping`, `buddyTooltip`, `visibleSections` (Online/Offline vs Buddies, Invitations only when there are some), `formatSize`, `describeUpload`, `describeCount`.

## The windows

### Buddy List (`routes/+page.svelte`)

The main window. It watches sync status, the Buddy List, recovery, trust and your presence, and renders:
- **Groups** from `visibleSections(buddies)`: each a collapsible `<details class="aim-group">` with the rooms in it (Buddy Icon or default icon, bold when unread, styled for away and offline, hover text from `buddyTooltip`).
- **Invitations** with Accept and Decline (`accept_invite` / `decline_invite`); the room moves on the next `buddylist:update`.
- **Double-click** opens a room: `invoke("open_room", { roomId })`.
- **Buttons** along the bottom: Add Buddy… (`open_add_buddy`), Away… (`open_away`, disabled without presence or while away), Preferences… (`open_preferences`), Sign Off.
- **The away banner** while you're away, with Edit… (`open_away`) and I'm Back (`set_away` with `message: null`).
- **Prompts** about encryption: Verify… (`verify_device`), Restore… (`open_recovery`), Set Up… (`open_setup_recovery`), and **the status bar** (`describe(status)`).
- **Sign Off** calls `logout`; Rust opens Sign On and closes this window.
- In dev builds, a **Style demo** link to `/demo`.

### Sign On (`routes/signon/`)

A server picker (matrix.org, plus the local dev servers in dev builds, plus a custom address), remembered in `localStorage` under `aimtrix.signon.server`. As you type a server, a debounced `sign_on_methods` decides what to show: screen name and password, **Use Browser…**, or only **Sign On with Browser…** for servers without passwords. `login` or `login_with_browser` signs on; on success Rust opens the Buddy List and closes this window, so the page never navigates itself. Cancel calls `cancel_browser_login`. `?signed-out` in the URL shows that the server ended the session.

### IM (`routes/im/`) and Chat Room (`routes/chat/`)

Both read `?room=` and are built from the same two components:
- **`Transcript.svelte`** (props: `roomId`, `unread`, and bindable `loading`, `saving`, `notice`, `error`) runs `watchTimeline` and renders each `Line`: names in your color or theirs, formatted HTML, images (thumbnails via `image_thumbnail`, loaded once each), files with Save (`save_attachment`), replies, "(edited)" and "(not sent)", deleted and undecryptable placeholders, membership and topic lines. It follows new messages unless you've scrolled up, keeps your place when older messages load above, loads more when you scroll to the top (`load_older`; `start` means there's no more), and marks the room read while the window has focus (`mark_read`, debounced). Conversation fonts follow Preferences (`applyFonts` sets `--font-conversation` and `--font-size-conversation`).
- **`Compose.svelte`** (props: `roomId`, `error`, `uploading`, and snippets `beside` and `actions`) is a `contenteditable` editor with Bold, Italic, Underline and a color palette, plus Send File…. Enter sends; Shift+Enter or Alt+Enter starts a new line. `serialize()` from `compose.ts` turns the editor into `{ body, html }`, keeping only bold, italic, underline, color and line breaks, and `html` is `null` when nothing is formatted. Typing notices start when you type, renew while you keep typing, and stop when you send, clear or pause (`set_typing`). Pasted pictures go to `send_pasted` (raw bytes); where WebKitGTK withholds them, `paste_picture` has Rust read the clipboard.

The IM window adds the buddy's and your Buddy Icons (`room_icon`, `my_icon`), their away message, and Block/Unblock (`block_buddy` asks first, `unblock_buddy`). The Chat Room window adds the member list and topic (`watchChatRoom`). Both watch the Buddy List to keep the name and unread state current, show upload progress in the status bar (`statusText` in `conversation/status.ts`), accept dropped files (`watchFileDrops` → `send_files` with paths), and can cancel an upload (`cancel_upload`).

**Received HTML is rendered with `{@html}`.** It's safe because Rust sanitized it (`matrix/html.rs`) before it ever reached the window. Don't render HTML from anywhere else this way. The `mxFormatting` action then applies Matrix's `data-mx-color` and `data-mx-bg-color` (validated as `#rrggbb`) and stops links navigating the window (opening links is a known issue in PLAN.md).

### The dialogs

| Route | Does | Calls |
|---|---|---|
| `away/` | Write an away message and go away; links to Preferences for auto-reply | `own_presence`/`presence:update`, `set_away`, `get_preferences`, `open_preferences` |
| `add-buddy/` | Start a DM by Matrix ID or bare name, then open it | `add_buddy`, `open_room` |
| `preferences/` | Auto-reply, tray, text color, font and size; About… | `get_preferences`, `set_preferences`, `tray_available`, `open_about` |
| `about/` | Version, license, third-party licenses | `open_license("license" \| "third-party")` |
| `recovery/` | Enter the recovery key or passphrase | `recover`, `recovery_status` |
| `setup-recovery/` | Make a recovery key, shown once, with a copy button and the right shortcut hint (`shortcutKey`) | `setup_recovery_check`, `setup_recovery`, `verify_device` |
| `verify/` | Follow the emoji comparison step by step | `verification_step`/`verification:step`, `accept_verification`, `start_verification`, `confirm_verification`. Closing the window cancels an unfinished verification in Rust. |

## Styling: `aim-chrome.css`

Hand-written CSS for a Win98/XP-era look, loaded once by the layout. Everything uses CSS custom properties on `:root`: the 3D palette (`--c-face`, `--c-hilite`, `--c-shadow`, `--c-dark`…), title bar gradients, conversation colors (`--c-im-self` red, `--c-im-buddy` blue), and fonts (`--font-ui`, `--font-conversation`, `--font-size-conversation`). Bevels are box-shadows and glyphs are inline SVG, so there are no image assets.

The components are BEM-style classes prefixed `aim-`: `aim-window`, `aim-frame`, `aim-titlebar`, `aim-menubar`, `aim-toolbar`, `aim-btn`, `aim-field`, `aim-label`, `aim-groupbox`, `aim-panel`, `aim-list`, `aim-group`, `aim-buddy-icon`, `aim-transcript`, `aim-msg`, `aim-palette`, `aim-statusbar`, `aim-alert`, `aim-away-banner`, `aim-drop-target`, `aim-sep`. Modifiers look like `aim-btn is-default` or `aim-titlebar is-inactive`.

**Rules:** reuse these classes and tokens rather than one-off styles, and when you add a new piece, show it in **`/demo`**. That route is a static showcase of every component with mock data and no Tauri calls, so it also opens in a plain browser at `http://localhost:1420/demo` while `npm run dev` runs. All art is original: no AOL logos, mascots or sounds.

## Recipes

### Add a window
1. Add the route: `src/routes/<name>/+page.svelte`. The layout gives it the frame and title bar for free.
2. Add the Rust side: an `open_*` in `windows.rs` and the label in `capabilities/default.json` (see [Backend: add a window](backend.md#add-a-window)).
3. Use existing `aim-` classes; add any new piece to `/demo`.

### Show a new piece of Matrix data
1. Add it to the Rust payload (or a new command or event); see [Backend recipes](backend.md#recipes).
2. Update the mirror type in `src/lib/*.ts`, and add a `watch…` function if it changes over time.
3. Use it in the route with `$effect(() => watchX(...))`.

### Type-check
`npm run check` (svelte-check). CI runs it on every platform.
