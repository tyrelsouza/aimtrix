<!-- The Buddy List: the main window. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { describe, watchSyncStatus, type SyncStatus } from "$lib/sync";
  import { watchRecovery, type Recovery } from "$lib/recovery";
  import { watchTrust, type Trust } from "$lib/verify";
  import { buddyTooltip, visibleSections, watchBuddyList, type Buddy } from "$lib/buddylist";
  import { watchOwnPresence, type OwnPresence } from "$lib/presence";
  import { DEFAULT_CHAT_ICON, DEFAULT_DM_ICON } from "$lib/icons";

  let userId = $state<string | null>(null);
  let status = $state<SyncStatus>({ state: "connecting" });
  let buddies = $state<Buddy[]>([]);
  let recovery = $state<Recovery>("unknown");
  let trust = $state<Trust>("unknown");
  let own = $state<OwnPresence>({ supported: null, away: null });
  let selected = $state<string | null>(null);
  /** Invitations being accepted or declined. */
  let responding = $state<string[]>([]);
  let busy = $state(false);
  let error = $state("");

  invoke<string | null>("whoami").then((id) => (userId = id));
  $effect(() => watchSyncStatus((s) => (status = s)));
  $effect(() => watchBuddyList((b) => (buddies = b)));
  $effect(() => watchRecovery((r) => (recovery = r)));
  $effect(() => watchTrust((t) => (trust = t)));
  $effect(() => watchOwnPresence((p) => (own = p)));

  async function comeBack() {
    error = "";
    try {
      await invoke("set_away", { message: null });
    } catch (e) {
      error = String(e);
    }
  }

  async function verify() {
    error = "";
    try {
      await invoke("verify_device");
    } catch (e) {
      error = String(e);
    }
  }

  const groups = $derived(
    visibleSections(buddies).map(([section, title]) => ({
      section,
      title,
      rooms: buddies.filter((b) => b.section === section),
    })),
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

  async function openRoom(room: Buddy) {
    // Invitations open once accepted.
    if (room.section === "invitations") return;
    error = "";
    try {
      await invoke("open_room", { roomId: room.roomId });
    } catch (e) {
      error = String(e);
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
                  class:is-away={room.presence === "away"}
                  class:is-offline={room.presence === "offline"}
                  class:is-blocked={room.blocked}
                  title={buddyTooltip(room)}
                  onclick={() => (selected = room.roomId)}
                  ondblclick={() => openRoom(room)}
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
    {#if own.away}
      <p class="away">
        <span class="grow" title={own.away}>You're away: {own.away}</span>
        <button class="aim-btn aim-btn--small" onclick={() => invoke("open_away")}>Edit…</button>
        <button class="aim-btn aim-btn--small" onclick={comeBack}>I'm Back</button>
      </p>
    {/if}
    {#if trust === "unverified" || recovery === "incomplete" || recovery === "disabled"}
      <p class="security">
        <span class="grow">
          🔒 {trust === "unverified"
            ? "This computer isn't verified."
            : recovery === "incomplete"
              ? "Older messages are locked."
              : "Recovery isn't set up."}
        </span>
        {#if trust === "unverified"}
          <button class="aim-btn aim-btn--small" onclick={verify}>Verify…</button>
        {/if}
        {#if recovery === "incomplete"}
          <button class="aim-btn aim-btn--small" onclick={() => invoke("open_recovery")}>Restore…</button>
        {:else if recovery === "disabled" && trust !== "unverified"}
          <button class="aim-btn aim-btn--small" onclick={() => invoke("open_setup_recovery")}>Set Up…</button>
        {/if}
      </p>
    {/if}
    <div class="buttons">
      <span class="row">
        <button class="aim-btn" onclick={() => invoke("open_add_buddy")}>Add Buddy…</button>
        <button
          class="aim-btn"
          onclick={() => invoke("open_away")}
          disabled={own.supported !== true || own.away !== null}
          title={own.supported === false ? "This server doesn't support away messages." : undefined}
        >
          Away…
        </button>
        <button class="aim-btn" onclick={signOff} disabled={busy}>Sign Off</button>
      </span>
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
  .security,
  .away {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .grow {
    flex: 1;
  }
  .away .grow {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row {
    display: flex;
    gap: 4px;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
  }
  .buttons .aim-btn {
    min-width: 0;
  }
</style>
