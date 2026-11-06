<!-- An IM window for one room. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import { page } from "$app/state";
  import { watchBuddyList } from "$lib/buddylist";
  import { watchTimeline, type Line } from "$lib/timeline";

  const roomId = page.url.searchParams.get("room") ?? "";
  let name = $state("");
  let lines = $state<Line[]>([]);
  let error = $state("");
  let draft = $state("");
  let transcript: HTMLDivElement;
  let box: HTMLTextAreaElement;

  /** Loading older messages. */
  let loading = $state(false);
  /** Set once `load_older` says there's no more history. */
  let noMore = $state(false);

  const shown = $derived(lines.filter((l) => l.type === "message" || l.type === "start"));
  const atStart = $derived(noMore || lines.some((l) => l.type === "start"));

  // Keep the name current if the room is renamed.
  $effect(() =>
    watchBuddyList((buddies) => {
      name = buddies.find((b) => b.roomId === roomId)?.name ?? name;
    }),
  );

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
    if (e.key !== "Enter" || e.isComposing || e.shiftKey) return;
    e.preventDefault();
    if (e.altKey) {
      box.setRangeText("\n", box.selectionStart, box.selectionEnd, "end");
      draft = box.value;
    } else {
      send();
    }
  }

  async function send(e?: SubmitEvent) {
    e?.preventDefault();
    const body = draft;
    if (!body.trim()) return;
    // Clear at once: the message shows up in the transcript straight away.
    draft = "";
    error = "";
    box.focus();
    try {
      await invoke("send_message", { roomId, body });
    } catch (err) {
      error = String(err);
      // Give the text back to retry, unless something new was typed.
      if (!draft) draft = body;
    }
  }

  function time(timestamp: number) {
    return new Date(timestamp).toLocaleString();
  }
</script>

<div class="layout">
  <main class="aim-window__body">
    <div class="aim-panel aim-transcript" bind:this={transcript} onscroll={scrolled}>
      {#each shown as line (line.id)}
        {#if line.type === "start"}
          <p class="aim-msg aim-msg--system">This is the beginning of the conversation.</p>
        {:else if line.type === "message"}
          <!-- One line: the paragraph keeps whitespace, so markup spacing would show. -->
          <p class="aim-msg" title={time(line.timestamp)}><span class="aim-msg__name" class:aim-msg__name--self={line.own} class:aim-msg__name--buddy={!line.own}>{line.emote ? `* ${line.name}` : `${line.name}:`}</span> {line.body}{#if line.failed}<span class="aim-msg__note"> (not sent)</span>{/if}</p>
        {/if}
      {/each}
    </div>
    {#if error}
      <p class="aim-alert" role="alert">{error}</p>
    {/if}
    <form class="compose" onsubmit={send}>
      <!-- svelte-ignore a11y_autofocus -->
      <textarea
        class="aim-field"
        rows="3"
        aria-label="Message"
        autofocus
        bind:value={draft}
        bind:this={box}
        onkeydown={keydown}
      ></textarea>
      <div class="buttons">
        <button class="aim-btn is-default" disabled={!draft.trim()}>Send</button>
      </div>
    </form>
  </main>
  <footer class="aim-statusbar">
    <!-- Here, not above the messages, so the text doesn't jump. -->
    <span class="aim-statusbar__section" role="status">
      {loading ? "Loading older messages…" : name}
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
  .aim-transcript {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .aim-msg {
    /* Keep the line breaks people type. */
    white-space: pre-wrap;
  }
  .compose {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  textarea {
    resize: none;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
  }
</style>
