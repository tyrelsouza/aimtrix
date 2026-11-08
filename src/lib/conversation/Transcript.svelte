<!--
  A room's messages, shared by the IM and Chat Room windows. Loads older
  messages as you scroll up, saves attachments, and marks the room read
  while its window has focus.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import {
    describeMemberChange,
    describeUndecryptable,
    formatSize,
    watchTimeline,
    type Line,
    type Media,
  } from "$lib/timeline";
  import { mxFormatting } from "$lib/compose";
  import { applyFonts, watchPreferences } from "$lib/preferences";

  let {
    roomId,
    unread,
    loading = $bindable(false),
    saving = $bindable(null),
    notice = $bindable(""),
    error = $bindable(""),
  }: {
    roomId: string;
    /** Has messages you haven't read, per the Buddy List. */
    unread: boolean;
    /** Loading older messages. */
    loading?: boolean;
    /** The attachment being saved, by name. */
    saving?: string | null;
    /** A short-lived status bar message, e.g. "Saved notes.txt". */
    notice?: string;
    error?: string;
  } = $props();

  let lines = $state<Line[]>([]);
  let transcript: HTMLDivElement;
  /** Set once `load_older` says there's no more history. */
  let noMore = $state(false);

  const shown = $derived(lines.filter((l) => l.type !== "other"));
  const atStart = $derived(noMore || lines.some((l) => l.type === "start"));

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

  let focused = $state(document.hasFocus());

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

  function time(timestamp: number) {
    return new Date(timestamp).toLocaleString();
  }

  // The whole window (transcript and message box) follows Preferences.
  $effect(() => watchPreferences(applyFonts));
</script>

<svelte:window onfocus={() => (focused = true)} onblur={() => (focused = false)} />

<div class="aim-panel aim-transcript" bind:this={transcript} onscroll={scrolled}>
  {#each shown as line (line.id)}
    {#if line.type === "start"}
      <p class="aim-msg aim-msg--system">This is the beginning of the conversation.</p>
    {:else if line.type === "membership"}
      <p class="aim-msg aim-msg--system" title={time(line.timestamp)}>{describeMemberChange(line.name, line.change)}</p>
    {:else if line.type === "topic"}
      <p class="aim-msg aim-msg--system" title={time(line.timestamp)}>
        {line.topic === null ? `${line.name} removed the topic.` : `${line.name} changed the topic to: ${line.topic}`}
      </p>
    {:else if line.type === "deleted"}
      <div class="aim-msg" title={time(line.timestamp)}>
        <span class="aim-msg__name" class:aim-msg__name--self={line.own} class:aim-msg__name--buddy={!line.own}>{line.name}:</span>
        <span class="aim-msg__note">(message deleted)</span>
      </div>
    {:else if line.type === "undecryptable"}
      <div class="aim-msg aim-msg--undecryptable" title={time(line.timestamp)}>
        <span class="aim-msg__name" class:aim-msg__name--self={line.own} class:aim-msg__name--buddy={!line.own}>{line.name}:</span>
        🔒 {describeUndecryptable(line.reason)}
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
        <span class="aim-msg__name" class:aim-msg__name--self={line.own} class:aim-msg__name--buddy={!line.own} title={line.sender}>{line.emote ? `* ${line.name}` : `${line.name}:`}</span>
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

<style>
  .aim-transcript {
    flex: 1;
    min-width: 0;
    overflow-y: auto;
  }
  .aim-msg__text {
    /* Keep the line breaks people type. */
    white-space: pre-wrap;
  }
</style>
