<!-- An IM window for one room. Messages come in 3.2. -->
<script lang="ts">
  import { page } from "$app/state";
  import { watchBuddyList } from "$lib/buddylist";

  const roomId = page.url.searchParams.get("room") ?? "";
  let name = $state("");

  // Keep the name current if the room is renamed.
  $effect(() =>
    watchBuddyList((buddies) => {
      name = buddies.find((b) => b.roomId === roomId)?.name ?? name;
    }),
  );
</script>

<div class="layout">
  <main class="aim-window__body">
    <p>{name}</p>
  </main>
  <footer class="aim-statusbar">
    <span class="aim-statusbar__section">{roomId}</span>
  </footer>
</div>

<style>
  .layout {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  p {
    margin: 0;
  }
</style>
