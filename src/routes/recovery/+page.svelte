<!-- Restore Messages: the recovery key lets this computer read old encrypted messages. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { tick } from "svelte";
  import { watchRecovery, type Recovery } from "$lib/recovery";

  let secret = $state("");
  let busy = $state(false);
  let error = $state("");
  let recovery = $state<Recovery>("unknown");
  // Set while the form is shown.
  let secretInput = $state<HTMLInputElement>();

  $effect(() => watchRecovery((r) => (recovery = r)));

  async function restore(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = "";
    try {
      await invoke("recover", { secret });
      // Don't keep the key around once it's used.
      secret = "";
      recovery = "enabled";
    } catch (e) {
      error = String(e);
      busy = false;
      await tick();
      secretInput?.select();
    }
  }

  function close() {
    getCurrentWebviewWindow().close();
  }
</script>

{#if recovery === "enabled"}
  <div class="aim-window__body">
    <p>Your messages are restored. Older encrypted messages will appear as their keys download.</p>
    <div class="actions">
      <!-- svelte-ignore a11y_autofocus -->
      <button class="aim-btn is-default" onclick={close} autofocus>Close</button>
    </div>
  </div>
{:else}
  <form class="aim-window__body" onsubmit={restore}>
    <p>🔒 This computer can't read your older encrypted messages yet.</p>
    <fieldset class="aim-groupbox" disabled={busy}>
      <legend>Recovery key</legend>
      <label class="aim-label">
        Enter the recovery key or passphrase you saved when you set up recovery (for example in Element).
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="aim-field"
          type="password"
          bind:this={secretInput}
          bind:value={secret}
          autocomplete="off"
          spellcheck="false"
          autofocus
          required
        />
      </label>
    </fieldset>

    {#if error}
      <p class="aim-alert" role="alert">{error}</p>
    {:else if busy}
      <p role="status">Restoring…</p>
    {:else if recovery === "disabled"}
      <p class="aim-alert" role="alert">This account has no recovery set up.</p>
    {/if}

    <div class="actions">
      <button class="aim-btn" type="button" onclick={close}>Not Now</button>
      <button class="aim-btn is-default" type="submit" disabled={busy}>Restore</button>
    </div>
  </form>
{/if}

<style>
  .aim-window__body {
    height: 100%;
  }
  p {
    margin: 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: auto;
  }
</style>
