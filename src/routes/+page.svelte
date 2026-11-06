<!-- The Buddy List: the main window. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { describe, watchSyncStatus, type SyncStatus } from "$lib/sync";
  import { SECTIONS, watchBuddyList, type Buddy } from "$lib/buddylist";
  import { DEFAULT_CHAT_ICON, DEFAULT_DM_ICON } from "$lib/icons";

  let userId = $state<string | null>(null);
  let status = $state<SyncStatus>({ state: "connecting" });
  let buddies = $state<Buddy[]>([]);
  let selected = $state<string | null>(null);
  /** Invitations being accepted or declined. */
  let responding = $state<string[]>([]);
  let busy = $state(false);
  let error = $state("");

  invoke<string | null>("whoami").then((id) => (userId = id));
  $effect(() => watchSyncStatus((s) => (status = s)));
  $effect(() => watchBuddyList((b) => (buddies = b)));

  // Invitations only show up when there are some.
  const groups = $derived(
    SECTIONS.map(([section, title]) => ({
      section,
      title,
      rooms: buddies.filter((b) => b.section === section),
    })).filter((g) => g.section !== "invitations" || g.rooms.length > 0),
  );

  async function respond(command: "accept_invite" | "decline_invite", roomId: string) {
    responding = [...responding, roomId];
    error = "";
    try {
      // The room leaves Invitations on the next buddylist:update.
      await invoke(command, { roomId });
    } catch (e) {
      error = String(e);
    } finally {
      responding = responding.filter((id) => id !== roomId);
    }
  }

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
    <div class="aim-panel">
      <!-- Keyed by section so a list update keeps each group's open state. -->
      {#each groups as group (group.section)}
        <details class="aim-group" open>
          <summary>
            {group.title} <span class="aim-group__count">({group.rooms.length})</span>
          </summary>
          <ul class="aim-list">
            {#each group.rooms as room (room.roomId)}
              <li>
                <button
                  class="aim-list__item"
                  class:is-selected={room.roomId === selected}
                  class:is-unread={room.unread}
                  onclick={() => (selected = room.roomId)}
                >
                  <img
                    class="aim-list__icon"
                    src={room.icon ?? (room.kind === "dm" ? DEFAULT_DM_ICON : DEFAULT_CHAT_ICON)}
                    alt=""
                  />
                  {room.name}
                </button>
                {#if room.section === "invitations"}
                  <div class="invite-actions">
                    <button
                      class="aim-btn aim-btn--small"
                      disabled={responding.includes(room.roomId)}
                      onclick={() => respond("accept_invite", room.roomId)}
                    >
                      Accept
                    </button>
                    <button
                      class="aim-btn aim-btn--small"
                      disabled={responding.includes(room.roomId)}
                      onclick={() => respond("decline_invite", room.roomId)}
                    >
                      Decline
                    </button>
                  </div>
                {/if}
              </li>
            {/each}
          </ul>
        </details>
      {/each}
    </div>
    {#if error}
      <p class="aim-alert" role="alert">{error}</p>
    {/if}
    <div class="buttons">
      <button class="aim-btn" onclick={signOff} disabled={busy}>Sign Off</button>
      {#if import.meta.env.DEV}
        <a href="/demo">Style demo</a>
      {/if}
    </div>
  </main>
  <footer class="aim-statusbar">
    <span
      class="aim-statusbar__section"
      role="status"
      title={status.state === "error" ? status.message : undefined}
    >
      {describe(status)}
    </span>
    <span class="aim-statusbar__section aim-statusbar__section--fit">{userId ?? ""}</span>
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
  .invite-actions {
    display: flex;
    gap: 4px;
    /* Line up with the room name, past the icon. */
    padding: 1px 0 3px 38px;
  }
  .buttons {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
</style>
