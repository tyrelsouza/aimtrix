<!-- Add Buddy: start a DM by Matrix ID. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { tick } from "svelte";

  /** Mirrors `Added` in src-tauri/src/matrix/rooms.rs. */
  type Added = { roomId: string; existing: boolean };

  let buddy = $state("");
  let busy = $state(false);
  let error = $state("");
  let buddyInput = $state<HTMLInputElement>();

  async function add(event: SubmitEvent) {
    event.preventDefault();
    // A double-click or a second Enter before the button greys out.
    if (busy) return;
    busy = true;
    error = "";
    try {
      const added = await invoke<Added>("add_buddy", { buddy });
      // Already a buddy: open the conversation you have instead.
      if (added.existing) await invoke("open_room", { roomId: added.roomId });
      close();
    } catch (e) {
      error = String(e);
      busy = false;
      await tick();
      buddyInput?.select();
    }
  }

  function close() {
    getCurrentWebviewWindow().close();
  }
</script>

<form class="aim-window__body" onsubmit={add}>
  <label class="aim-label">
    Screen name or Matrix ID of the buddy to add:
    <!-- svelte-ignore a11y_autofocus -->
    <input
      class="aim-field"
      bind:this={buddyInput}
      bind:value={buddy}
      placeholder="@name:matrix.org"
      disabled={busy}
      autocomplete="off"
      spellcheck="false"
      autofocus
      required
    />
  </label>
  <p class="hint">They get an invitation, and show up under Buddies.</p>

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {:else if busy}
    <p role="status">Adding…</p>
  {/if}

  <div class="actions">
    <button class="aim-btn" type="button" onclick={close}>Cancel</button>
    <button class="aim-btn is-default" type="submit" disabled={busy}>Add</button>
  </div>
</form>

<style>
  .aim-window__body {
    height: 100%;
  }
  p {
    margin: 0;
  }
  .hint {
    color: var(--c-text-disabled);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: auto;
  }
</style>
