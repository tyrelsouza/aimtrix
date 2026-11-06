<!-- Buddy List placeholder: a plain-text list until step 2.3. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { describe, watchSyncStatus, type SyncStatus } from "$lib/sync";
  import { SECTIONS, watchBuddyList, type Buddy } from "$lib/buddylist";

  let userId = $state<string | null>(null);
  let status = $state<SyncStatus>({ state: "connecting" });
  let buddies = $state<Buddy[]>([]);
  let busy = $state(false);
  let error = $state("");

  invoke<string | null>("whoami").then((id) => (userId = id));
  $effect(() => watchSyncStatus((s) => (status = s)));
  $effect(() => watchBuddyList((b) => (buddies = b)));

  async function signOff() {
    busy = true;
    error = "";
    try {
      // On success Rust opens Sign On and closes this window.
      await invoke("logout");
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }
</script>

<div class="layout">
  <main class="aim-window__body">
    <p>Signed on as {userId ?? "…"}</p>
    <div class="rooms">
      {#each SECTIONS as [section, title] (section)}
        {@const rooms = buddies.filter((b) => b.section === section)}
        <p>{title} ({rooms.length})</p>
        <ul>
          {#each rooms as room (room.roomId)}
            <li>{room.name}{room.kind === "group" ? " [chat]" : ""}</li>
          {/each}
        </ul>
      {/each}
    </div>
    {#if error}
      <p class="aim-alert" role="alert">{error}</p>
    {/if}
    <div>
      <button class="aim-btn" onclick={signOff} disabled={busy}>Sign Off</button>
    </div>
    {#if import.meta.env.DEV}
      <p><a href="/demo">Style demo</a></p>
    {/if}
  </main>
  <footer class="aim-statusbar">
    <span
      class="aim-statusbar__section"
      role="status"
      title={status.state === "error" ? status.message : undefined}
    >
      {describe(status)}
    </span>
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
  .rooms {
    flex: 1;
    overflow: auto;
  }
  ul {
    margin: 0 0 4px;
    padding-left: 16px;
  }
</style>
