<!-- Away Message: type an away message and go away. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import {
    DEFAULT_AWAY_MESSAGE,
    MAX_AWAY_MESSAGE,
    watchOwnPresence,
    type OwnPresence,
  } from "$lib/presence";

  let message = $state(DEFAULT_AWAY_MESSAGE);
  let own = $state<OwnPresence>({ supported: null, away: null });
  let busy = $state(false);
  let error = $state("");
  let filled = false;

  $effect(() =>
    watchOwnPresence((p) => {
      own = p;
      // Start from the current away message when editing it.
      if (!filled && p.away) message = p.away;
      filled = true;
    }),
  );

  async function goAway(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = "";
    try {
      await invoke("set_away", { message });
      close();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }

  function close() {
    getCurrentWebviewWindow().close();
  }
</script>

<form class="aim-window__body" onsubmit={goAway}>
  <label class="aim-label grow">
    Away message, shown to buddies while you're away:
    <!-- svelte-ignore a11y_autofocus -->
    <textarea
      class="aim-field grow"
      bind:value={message}
      maxlength={MAX_AWAY_MESSAGE}
      disabled={busy || own.supported === false}
      autofocus
      required
    ></textarea>
  </label>

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {:else if own.supported === false}
    <p class="aim-alert" role="alert">This server doesn't support away messages.</p>
  {:else if own.supported === null}
    <p role="status">Checking the server…</p>
  {/if}

  <div class="actions">
    <button class="aim-btn" type="button" onclick={close}>Cancel</button>
    <button class="aim-btn is-default" type="submit" disabled={busy || own.supported !== true}>
      {own.away ? "Update" : "I'm Away"}
    </button>
  </div>
</form>

<style>
  .aim-window__body {
    height: 100%;
  }
  p {
    margin: 0;
  }
  .grow {
    flex: 1;
    min-height: 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
