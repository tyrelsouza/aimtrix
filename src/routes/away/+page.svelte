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
  import { watchPreferences } from "$lib/preferences";

  let message = $state(DEFAULT_AWAY_MESSAGE);
  let own = $state<OwnPresence>({ supported: null, away: null });
  let busy = $state(false);
  let error = $state("");
  let filled = false;
  let autoReply = $state(false);
  /**
   * Characters as Rust counts them (code points), after trimming. The
   * textarea's `maxlength` would count an emoji as two.
   */
  const length = $derived([...message.trim()].length);
  const tooLong = $derived(length > MAX_AWAY_MESSAGE);

  $effect(() => watchPreferences((p) => (autoReply = p.autoReply)));

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
      disabled={busy || own.supported === false}
      autofocus
      required
    ></textarea>
  </label>

  {#if own.supported !== false}
    <p class="auto-reply">
      <span>Auto-reply is {autoReply ? "on: IMs get this message once" : "off"}.</span>
      <button type="button" class="aim-btn aim-btn--small" onclick={() => invoke("open_preferences")}>
        Preferences…
      </button>
    </p>
  {/if}

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {:else if tooLong}
    <p class="aim-alert" role="alert">
      Away messages can be up to {MAX_AWAY_MESSAGE} characters; this one is {length}.
    </p>
  {:else if own.supported === false}
    <p class="aim-alert" role="alert">This server doesn't support away messages.</p>
  {:else if own.supported === null}
    <p role="status">Checking the server…</p>
  {/if}

  <div class="actions">
    <button class="aim-btn" type="button" onclick={close}>Cancel</button>
    <button class="aim-btn is-default" type="submit" disabled={busy || own.supported !== true || tooLong}>
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
  .auto-reply {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 4px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
  }
</style>
