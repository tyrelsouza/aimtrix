<!-- A Chat Room window for a room with several people. -->
<script lang="ts">
  import { page } from "$app/state";
  import { watchBuddyList } from "$lib/buddylist";
  import { watchTyping } from "$lib/timeline";
  import { describeCount, watchChatRoom, type Member } from "$lib/chatroom";
  import { DEFAULT_DM_ICON } from "$lib/icons";
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
  /** Null until loaded. */
  let members = $state<Member[] | null>(null);
  let topic = $state<string | null>(null);

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

  $effect(() =>
    watchChatRoom(
      roomId,
      (m) => (members = m),
      (t) => (topic = t),
      (e) => (error = e),
    ),
  );
</script>

<div class="layout">
  <main class="aim-window__body">
    {#if topic}
      <p class="topic" title={topic}><b>Topic:</b> {topic}</p>
    {/if}
    <div class="row grow">
      <Transcript {roomId} {unread} bind:loading bind:saving bind:notice bind:error />
      <section class="members" aria-label="People in this room">
        <span>{members ? describeCount(members.length) : "Loading…"}</span>
        <ul class="aim-panel aim-list">
          {#each members ?? [] as member (member.userId)}
            <li class="aim-list__item" title={member.userId}>
              <img class="aim-list__icon" src={member.icon ?? DEFAULT_DM_ICON} alt="" />
              <span class="name">{member.name}</span>
            </li>
          {/each}
        </ul>
      </section>
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
  .topic {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .members {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 140px;
    flex-shrink: 0;
  }
  .members ul {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }
  .members li {
    padding-left: 4px;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
