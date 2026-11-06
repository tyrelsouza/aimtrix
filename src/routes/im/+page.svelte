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

  const messages = $derived(lines.filter((l) => l.type === "message"));

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
        // Follow new messages, unless you've scrolled up to read.
        const atBottom =
          transcript.scrollHeight - transcript.scrollTop - transcript.clientHeight < 8;
        lines = l;
        await tick();
        if (atBottom) transcript.scrollTop = transcript.scrollHeight;
      },
      (e) => (error = e),
    ),
  );

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
    <div class="aim-panel aim-transcript" bind:this={transcript}>
      {#each messages as msg (msg.id)}
        <!-- One line: the paragraph keeps whitespace, so markup spacing would show. -->
        <p class="aim-msg" title={time(msg.timestamp)}><span class="aim-msg__name" class:aim-msg__name--self={msg.own} class:aim-msg__name--buddy={!msg.own}>{msg.emote ? `* ${msg.name}` : `${msg.name}:`}</span> {msg.body}{#if msg.failed}<span class="aim-msg__note"> (not sent)</span>{/if}</p>
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
    <span class="aim-statusbar__section">{name}</span>
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
