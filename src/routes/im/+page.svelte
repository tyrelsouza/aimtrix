<!-- An IM window for one room. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { tick } from "svelte";
  import { page } from "$app/state";
  import { watchBuddyList } from "$lib/buddylist";
  import { describeTyping, watchTimeline, watchTyping, type Line } from "$lib/timeline";

  const roomId = page.url.searchParams.get("room") ?? "";
  let name = $state("");
  let lines = $state<Line[]>([]);
  let error = $state("");
  let draft = $state("");
  let transcript: HTMLDivElement;
  let box: HTMLTextAreaElement;

  /** Names of who's typing, not counting you. */
  let typing = $state<string[]>([]);
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

  $effect(() => watchTyping((names) => (typing = names)));

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
      typed();
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
    stopTyping();
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
        oninput={typed}
      ></textarea>
      <div class="buttons">
        <button class="aim-btn is-default" disabled={!draft.trim()}>Send</button>
      </div>
    </form>
  </main>
  <footer class="aim-statusbar">
    <!-- Here, not above the messages, so the text doesn't jump. -->
    <span class="aim-statusbar__section" role="status">
      {loading ? "Loading older messages…" : describeTyping(typing) || name}
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
