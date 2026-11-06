<!-- An IM window for one room. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import { page } from "$app/state";
  import { watchBuddyList } from "$lib/buddylist";
  import {
    describeTyping,
    formatSize,
    watchTimeline,
    watchTyping,
    type Line,
    type Media,
  } from "$lib/timeline";
  import { COLORS, mxFormatting, serialize } from "$lib/compose";
  import { DEFAULT_CHAT_ICON, DEFAULT_DM_ICON } from "$lib/icons";

  const roomId = page.url.searchParams.get("room") ?? "";
  let name = $state("");
  let lines = $state<Line[]>([]);
  let error = $state("");
  /** The editor's plain text, to know whether there's anything to send. */
  let draft = $state("");
  let transcript: HTMLDivElement;
  let editor: HTMLDivElement;
  /** Toolbar state at the cursor. */
  let bold = $state(false);
  let italic = $state(false);
  let underline = $state(false);
  let color = $state(COLORS[0][0]);
  let paletteOpen = $state(false);

  /** Names of who's typing, not counting you. */
  let typing = $state<string[]>([]);
  /** Loading older messages. */
  let loading = $state(false);
  /** Set once `load_older` says there's no more history. */
  let noMore = $state(false);

  const shown = $derived(lines.filter((l) => l.type !== "other"));

  /** Image thumbnails by event ID; null if it couldn't load. */
  let thumbnails = $state<Record<string, string | null>>({});
  const asked = new Set<string>();

  // Load each image's thumbnail once, as its message shows up.
  $effect(() => {
    for (const line of shown) {
      if (line.type !== "message" || line.media?.kind !== "image" || !line.eventId) continue;
      const eventId = line.eventId;
      if (asked.has(eventId)) continue;
      asked.add(eventId);
      invoke<string>("image_thumbnail", { roomId, eventId })
        .then((url) => (thumbnails[eventId] = url))
        .catch((e) => {
          console.warn("Couldn't load an image:", e);
          thumbnails[eventId] = null;
        });
    }
  });

  /** The attachment being saved, by name. */
  let saving = $state<string | null>(null);
  /** A short-lived status bar message, e.g. "Saved notes.txt". */
  let notice = $state("");
  let noticeTimer: ReturnType<typeof setTimeout> | undefined;

  async function saveAttachment(eventId: string, name: string) {
    saving = name;
    error = "";
    try {
      // Rust asks where to save it; null means you cancelled.
      const savedAs = await invoke<string | null>("save_attachment", { roomId, eventId });
      if (savedAs !== null) {
        notice = `Saved ${savedAs}`;
        clearTimeout(noticeTimer);
        noticeTimer = setTimeout(() => (notice = ""), 4000);
      }
    } catch (e) {
      error = String(e);
    } finally {
      saving = null;
    }
  }

  /** Following the newest message, i.e. scrolled to the bottom. */
  let pinned = true;

  /** A thumbnail loaded and grew its message: stay at the bottom if there. */
  function imageLoaded() {
    if (pinned) transcript.scrollTop = transcript.scrollHeight;
  }

  const MEDIA_ICONS: Record<Media["kind"], string> = {
    image: "🖼",
    video: "🎞",
    audio: "🔊",
    file: "📎",
  };
  const atStart = $derived(noMore || lines.some((l) => l.type === "start"));

  let kind = $state<"dm" | "group">("dm");
  /** Has messages you haven't read, per the Buddy List. */
  let unread = $state(false);
  let buddyIcon = $state<string | null>(null);
  let myIcon = $state<string | null>(null);
  /** The Buddy List's (small) icon, to notice when it changes. */
  let listIcon: string | null | undefined;
  let focused = $state(document.hasFocus());

  // Keep the name, icon and unread state current.
  $effect(() =>
    watchBuddyList((buddies) => {
      const buddy = buddies.find((b) => b.roomId === roomId);
      if (!buddy) return;
      name = buddy.name;
      kind = buddy.kind;
      unread = buddy.unread;
      if (buddy.icon !== listIcon) {
        listIcon = buddy.icon;
        loadBuddyIcon();
      }
    }),
  );

  invoke<string | null>("my_icon")
    .then((icon) => (myIcon = icon))
    .catch((e) => console.warn("Couldn't load your Buddy Icon:", e));

  async function loadBuddyIcon() {
    try {
      buddyIcon = await invoke<string | null>("room_icon", { roomId });
    } catch (e) {
      console.warn("Couldn't load the Buddy Icon:", e);
    }
  }

  // Mark the room read while you're looking at it: when the window gets
  // focus, and when messages arrive while it has it. Waits a moment so a
  // burst of updates sends one receipt.
  let readTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void shown.length;
    if (!focused || !unread) return;
    clearTimeout(readTimer);
    readTimer = setTimeout(() => {
      invoke("mark_read", { roomId }).catch((e) =>
        console.warn("Couldn't mark the conversation read:", e),
      );
    }, 300);
  });

  $effect(() =>
    watchTimeline(
      roomId,
      async (l) => {
        const atBottom =
          transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 8;
        const fromBottom = transcript.scrollHeight - transcript.scrollTop;
        const firstBefore = shown[0]?.id;
        lines = l;
        await tick();
        if (atBottom) {
          // Follow new messages, unless you've scrolled up to read.
          transcript.scrollTop = transcript.scrollHeight;
        } else if (shown[0]?.id !== firstBefore) {
          // Older messages came in above: keep what you were reading in place.
          transcript.scrollTop = transcript.scrollHeight - fromBottom;
        }
        fill();
      },
      (e) => (error = e),
    ),
  );

  $effect(() => watchTyping((names) => (typing = names)));

  // Ready to type when the window opens.
  $effect(() => editor.focus());

  // Your own typing notice: started when you type, renewed while you keep
  // typing, and stopped when you send, clear the box or pause. The server
  // drops it after a few seconds if it isn't renewed.
  const RENEW_EVERY = 2000;
  const IDLE_AFTER = 5000;
  let typingSent = false;
  let typingSentAt = 0;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;

  function typed() {
    clearTimeout(idleTimer);
    if (!draft.trim()) {
      stopTyping();
      return;
    }
    if (!typingSent || Date.now() - typingSentAt > RENEW_EVERY) {
      typingSent = true;
      typingSentAt = Date.now();
      setTyping(true);
    }
    // Stopped typing without sending: say so after a while.
    idleTimer = setTimeout(stopTyping, IDLE_AFTER);
  }

  function stopTyping() {
    clearTimeout(idleTimer);
    if (typingSent) {
      typingSent = false;
      setTyping(false);
    }
  }

  function setTyping(on: boolean) {
    // A lost typing notice isn't worth interrupting you for.
    invoke("set_typing", { roomId, typing: on }).catch((e) =>
      console.warn("Couldn't send the typing notice:", e),
    );
  }

  /** Load older messages when you scroll near the top. */
  function scrolled() {
    pinned = transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 8;
    if (transcript.scrollTop < 40) loadOlder();
  }

  /** Until the transcript can scroll, there's no scrolling up to load more. */
  function fill() {
    if (transcript.scrollHeight <= transcript.clientHeight + 40) loadOlder();
  }

  async function loadOlder() {
    if (loading || atStart) return;
    loading = true;
    try {
      noMore = await invoke<boolean>("load_older", { roomId });
    } catch (e) {
      error = String(e);
      // Don't retry in a loop; scrolling up tries again.
      return;
    } finally {
      loading = false;
    }
    await tick();
    fill();
  }

  /** Enter sends; Shift+Enter and Alt+Enter start a new line. */
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape" && paletteOpen) {
      paletteOpen = false;
      return;
    }
    if (e.key !== "Enter" || e.isComposing) return;
    e.preventDefault();
    if (e.shiftKey || e.altKey) {
      document.execCommand("insertLineBreak");
    } else {
      send();
    }
  }

  function edited() {
    draft = serialize(editor).body;
    typed();
  }

  /** Paste as plain text, so outside styles don't come along. */
  function paste(e: ClipboardEvent) {
    e.preventDefault();
    document.execCommand("insertText", false, e.clipboardData?.getData("text/plain") ?? "");
  }

  /** Keep the toolbar showing the formatting at the cursor. */
  function selectionChanged() {
    if (!editor?.contains(document.getSelection()?.anchorNode ?? null)) return;
    bold = document.queryCommandState("bold");
    italic = document.queryCommandState("italic");
    underline = document.queryCommandState("underline");
  }

  function format(command: "bold" | "italic" | "underline") {
    editor.focus();
    document.execCommand(command);
    selectionChanged();
    edited();
  }

  function pickColor(hex: string) {
    paletteOpen = false;
    color = hex;
    editor.focus();
    document.execCommand("foreColor", false, hex);
    edited();
  }

  async function send(e?: SubmitEvent) {
    e?.preventDefault();
    const { body, html } = serialize(editor);
    if (!body.trim()) return;
    const saved = editor.innerHTML;
    // Clear at once: the message shows up in the transcript straight away.
    editor.innerHTML = "";
    draft = "";
    stopTyping();
    error = "";
    editor.focus();
    try {
      await invoke("send_message", { roomId, body, html });
    } catch (err) {
      error = String(err);
      // Give the text back to retry, unless something new was typed.
      if (!draft) {
        editor.innerHTML = saved;
        draft = body;
      }
    }
  }

  function time(timestamp: number) {
    return new Date(timestamp).toLocaleString();
  }
</script>

<svelte:document onselectionchange={selectionChanged} />
<svelte:window onfocus={() => (focused = true)} onblur={() => (focused = false)} />

<div class="layout">
  <main class="aim-window__body">
    <div class="row grow">
      <div class="aim-panel aim-transcript" bind:this={transcript} onscroll={scrolled}>
        {#each shown as line (line.id)}
          {#if line.type === "start"}
            <p class="aim-msg aim-msg--system">This is the beginning of the conversation.</p>
          {:else if line.type === "deleted"}
            <div class="aim-msg" title={time(line.timestamp)}>
              <span class="aim-msg__name" class:aim-msg__name--self={line.own} class:aim-msg__name--buddy={!line.own}>{line.name}:</span>
              <span class="aim-msg__note">(message deleted)</span>
            </div>
          {:else if line.type === "message"}
            <div class="aim-msg" title={time(line.timestamp)}>
              {#if line.reply}
                <div class="aim-msg__reply">
                  {#if line.reply.name}
                    In reply to <b>{line.reply.name}</b>: {line.reply.preview}
                  {:else}
                    In reply to a message
                  {/if}
                </div>
              {/if}
              <!-- The HTML was cleaned by the Rust core. -->
              <span class="aim-msg__name" class:aim-msg__name--self={line.own} class:aim-msg__name--buddy={!line.own}>{line.emote ? `* ${line.name}` : `${line.name}:`}</span>
              {#if line.media}
                {@const media = line.media}
                {@const thumbnail = line.eventId ? thumbnails[line.eventId] : undefined}
                {@const eventId = line.eventId}
                <button
                  type="button"
                  class="aim-msg__attachment"
                  title="Save {media.name}…"
                  disabled={!eventId || saving !== null}
                  onclick={() => eventId && saveAttachment(eventId, media.name)}
                >
                  {#if media.kind === "image" && thumbnail}
                    <img
                      class="aim-msg__image"
                      src={thumbnail}
                      alt={media.name}
                      style:aspect-ratio={media.width && media.height ? `${media.width} / ${media.height}` : undefined}
                      onload={imageLoaded}
                    />
                  {:else}
                    <span class="aim-msg__file">{MEDIA_ICONS[media.kind]} {media.name}{#if media.size !== null}<span class="aim-msg__note"> ({formatSize(media.size)})</span>{/if}</span>
                  {/if}
                </button>
              {/if}
              {#if !line.media || line.body !== line.media.name}{#if line.html}<span class="aim-msg__html" use:mxFormatting={line.html}>{@html line.html}</span>{:else}<span class="aim-msg__text">{line.body}</span>{/if}{/if}{#if line.edited}<span class="aim-msg__note"> (edited)</span>{/if}{#if line.failed}<span class="aim-msg__note"> (not sent)</span>{/if}
            </div>
          {/if}
        {/each}
      </div>
      <img
        class="aim-buddy-icon"
        src={buddyIcon ?? (kind === "dm" ? DEFAULT_DM_ICON : DEFAULT_CHAT_ICON)}
        alt="{name}'s Buddy Icon"
      />
    </div>
    {#if error}
      <p class="aim-alert" role="alert">{error}</p>
    {/if}
    <form class="compose" onsubmit={send}>
      <!-- Buttons keep the editor's selection: mousedown would take focus. -->
      <div class="aim-toolbar" role="toolbar" aria-label="Formatting">
        <button type="button" class="aim-toolbar__btn" title="Bold" aria-pressed={bold}
          onmousedown={(e) => e.preventDefault()} onclick={() => format("bold")}><b>B</b></button>
        <button type="button" class="aim-toolbar__btn" title="Italic" aria-pressed={italic}
          onmousedown={(e) => e.preventDefault()} onclick={() => format("italic")}><i>I</i></button>
        <button type="button" class="aim-toolbar__btn" title="Underline" aria-pressed={underline}
          onmousedown={(e) => e.preventDefault()} onclick={() => format("underline")}><u>U</u></button>
        <span class="aim-toolbar__sep"></span>
        <div class="color">
          <button type="button" class="aim-toolbar__btn" title="Text color" aria-haspopup="true"
            aria-expanded={paletteOpen} style:color={color}
            onmousedown={(e) => e.preventDefault()} onclick={() => (paletteOpen = !paletteOpen)}>A</button>
          {#if paletteOpen}
            <div class="aim-palette" role="menu" aria-label="Text color">
              {#each COLORS as [hex, label] (hex)}
                <button type="button" class="aim-palette__swatch" role="menuitem" title={label}
                  aria-label={label} style:background={hex}
                  onmousedown={(e) => e.preventDefault()} onclick={() => pickColor(hex)}></button>
              {/each}
            </div>
          {/if}
        </div>
      </div>
      <div class="row">
        <div
          class="aim-field editor"
          contenteditable="true"
          role="textbox"
          tabindex="0"
          aria-multiline="true"
          aria-label="Message"
          bind:this={editor}
          onkeydown={keydown}
          oninput={edited}
          onpaste={paste}
          ondrop={(e) => e.preventDefault()}
        ></div>
        <img class="aim-buddy-icon" src={myIcon ?? DEFAULT_DM_ICON} alt="Your Buddy Icon" />
      </div>
      <div class="buttons">
        <button class="aim-btn is-default" disabled={!draft.trim()}>Send</button>
      </div>
    </form>
  </main>
  <footer class="aim-statusbar">
    <!-- Here, not above the messages, so the text doesn't jump. -->
    <span class="aim-statusbar__section" role="status">
      {loading
        ? "Loading older messages…"
        : saving !== null
          ? `Saving ${saving}…`
          : notice || describeTyping(typing) || name}
    </span>
  </footer>
</div>

<style>
  .layout {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  main {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .row.grow {
    flex: 1;
    min-height: 0;
  }
  .aim-transcript {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }
  .aim-msg__text {
    /* Keep the line breaks people type. */
    white-space: pre-wrap;
  }
  .compose {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .editor {
    flex: 1;
    min-width: 0;
    /* About three lines, then it scrolls. */
    height: 4.6em;
    overflow-y: auto;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .color {
    position: relative;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
  }
</style>
