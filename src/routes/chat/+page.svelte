<!-- A Chat Room window for a room with several people. -->
<script lang="ts">
  import { page } from "$app/state";
  import { watchBuddyList } from "$lib/buddylist";
  import { watchTyping } from "$lib/timeline";
  import Transcript from "$lib/conversation/Transcript.svelte";
  import Compose from "$lib/conversation/Compose.svelte";
  import { statusText } from "$lib/conversation/status";

  const roomId = page.url.searchParams.get("room") ?? "";
  let name = $state("");
  /** Has messages you haven't read, per the Buddy List. */
  let unread = $state(false);
  let error = $state("");
  let loading = $state(false);
  let saving = $state<string | null>(null);
  let notice = $state("");
  /** Names of who's typing, not counting you. */
  let typing = $state<string[]>([]);

  // Keep the name and unread state current.
  $effect(() =>
    watchBuddyList((buddies) => {
      const buddy = buddies.find((b) => b.roomId === roomId);
      if (!buddy) return;
      name = buddy.name;
      unread = buddy.unread;
    }),
  );

  $effect(() => watchTyping((names) => (typing = names)));
</script>

<div class="layout">
  <main class="aim-window__body">
    <!-- The member pane goes beside the transcript (step 4.2). -->
    <div class="row grow">
      <Transcript {roomId} {unread} bind:loading bind:saving bind:notice bind:error />
    </div>
    {#if error}
      <p class="aim-alert" role="alert">{error}</p>
    {/if}
    <Compose {roomId} bind:error />
  </main>
  <footer class="aim-statusbar">
    <span class="aim-statusbar__section" role="status">
      {statusText({ loading, saving, notice, typing, name })}
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
</style>
