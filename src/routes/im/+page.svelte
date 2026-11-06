<!-- An IM window for a one-to-one room. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { page } from "$app/state";
  import { watchBuddyList } from "$lib/buddylist";
  import { watchTyping } from "$lib/timeline";
  import { DEFAULT_CHAT_ICON, DEFAULT_DM_ICON } from "$lib/icons";
  import Transcript from "$lib/conversation/Transcript.svelte";
  import Compose from "$lib/conversation/Compose.svelte";
  import { statusText } from "$lib/conversation/status";

  const roomId = page.url.searchParams.get("room") ?? "";
  let name = $state("");
  let kind = $state<"dm" | "group">("dm");
  /** Has messages you haven't read, per the Buddy List. */
  let unread = $state(false);
  let error = $state("");
  let loading = $state(false);
  let saving = $state<string | null>(null);
  let notice = $state("");
  /** Names of who's typing, not counting you. */
  let typing = $state<string[]>([]);

  let buddyIcon = $state<string | null>(null);
  let myIcon = $state<string | null>(null);
  /** The Buddy List's (small) icon, to notice when it changes. */
  let listIcon: string | null | undefined;

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

  $effect(() => watchTyping((names) => (typing = names)));
</script>

<div class="layout">
  <main class="aim-window__body">
    <div class="row grow">
      <Transcript {roomId} {unread} bind:loading bind:saving bind:notice bind:error />
      <img
        class="aim-buddy-icon"
        src={buddyIcon ?? (kind === "dm" ? DEFAULT_DM_ICON : DEFAULT_CHAT_ICON)}
        alt="{name}'s Buddy Icon"
      />
    </div>
    {#if error}
      <p class="aim-alert" role="alert">{error}</p>
    {/if}
    <Compose {roomId} bind:error>
      {#snippet beside()}
        <img class="aim-buddy-icon" src={myIcon ?? DEFAULT_DM_ICON} alt="Your Buddy Icon" />
      {/snippet}
    </Compose>
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
