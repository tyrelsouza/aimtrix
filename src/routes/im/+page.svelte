<!-- An IM window for one room. -->
<script lang="ts">
  import { tick } from "svelte";
  import { page } from "$app/state";
  import { watchBuddyList } from "$lib/buddylist";
  import { watchTimeline, type Line } from "$lib/timeline";

  const roomId = page.url.searchParams.get("room") ?? "";
  let name = $state("");
  let lines = $state<Line[]>([]);
  let error = $state("");
  let transcript: HTMLDivElement;

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

  function time(timestamp: number) {
    return new Date(timestamp).toLocaleString();
  }
</script>

<div class="layout">
  <main class="aim-window__body">
    <div class="aim-panel aim-transcript" bind:this={transcript}>
      {#each messages as msg (msg.id)}
        <!-- One line: the paragraph keeps whitespace, so markup spacing would show. -->
        <p class="aim-msg" title={time(msg.timestamp)}><span class="aim-msg__name" class:aim-msg__name--self={msg.own} class:aim-msg__name--buddy={!msg.own}>{msg.emote ? `* ${msg.name}` : `${msg.name}:`}</span> {msg.body}</p>
      {/each}
    </div>
    {#if error}
      <p class="aim-alert" role="alert">{error}</p>
    {/if}
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
</style>
