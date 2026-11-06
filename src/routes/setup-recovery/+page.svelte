<!-- Set Up Recovery: makes a recovery key for an account that has none, and shows it once. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

  /** Mirrors `Setup` in src-tauri/src/matrix/crypto.rs. */
  type Setup = "ready" | "verify-first" | "already-set-up";

  let setup = $state<Setup | null>(null);
  let busy = $state(false);
  let error = $state("");
  // Only ever in this window: never logged or sent anywhere else.
  let key = $state("");
  let saved = $state(false);
  let copied = $state(false);
  let keyBox = $state<HTMLElement>();

  invoke<Setup>("setup_recovery_check")
    .then((s) => (setup = s))
    .catch((e) => (error = String(e)));

  async function setUp() {
    busy = true;
    error = "";
    try {
      key = await invoke<string>("setup_recovery");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(key);
      copied = true;
    } catch {
      // No clipboard access: select it so Ctrl+C works.
      const range = document.createRange();
      range.selectNodeContents(keyBox!);
      getSelection()?.removeAllRanges();
      getSelection()?.addRange(range);
      error = "Couldn't copy. The key is selected: press Ctrl+C.";
    }
  }

  async function verify() {
    try {
      await invoke("verify_device");
      close();
    } catch (e) {
      error = String(e);
    }
  }

  function close() {
    key = "";
    getCurrentWebviewWindow().close();
  }
</script>

<div class="aim-window__body">
  {#if key}
    <p>This is your recovery key. Write it down or save it somewhere safe, like a password manager:</p>
    <code class="aim-field key" bind:this={keyBox}>{key}</code>
    <p>You'll need it to read your encrypted messages on a new computer. AIMtrix won't show it again.</p>
    <label class="saved"><input type="checkbox" bind:checked={saved} /> I've saved my recovery key</label>
  {:else if setup === "ready"}
    <p>🔒 Protect your messages.</p>
    <p>
      If you lose access to all your sessions, a recovery key is the only way to read your encrypted messages again.
      AIMtrix will back up your message keys and make a recovery key for you.
    </p>
  {:else if setup === "verify-first"}
    <p>Your account's encryption was set up in another app, and this computer doesn't have its keys yet.</p>
    <p>Verify this computer with that app first, then set up recovery. (Or set up recovery in that app.)</p>
  {:else if setup === "already-set-up"}
    <p>Recovery is already set up for your account.</p>
  {:else if !error}
    <p>Checking…</p>
  {/if}

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {:else if busy}
    <p role="status">Setting up…</p>
  {/if}

  <div class="actions">
    {#if key}
      <button class="aim-btn" onclick={copy}>{copied ? "Copied" : "Copy"}</button>
      <button class="aim-btn is-default" onclick={close} disabled={!saved}>Done</button>
    {:else if setup === "ready"}
      <button class="aim-btn" onclick={close} disabled={busy}>Not Now</button>
      <button class="aim-btn is-default" onclick={setUp} disabled={busy}>Set Up</button>
    {:else if setup === "verify-first"}
      <button class="aim-btn" onclick={close}>Close</button>
      <button class="aim-btn is-default" onclick={verify}>Verify…</button>
    {:else}
      <button class="aim-btn is-default" onclick={close}>Close</button>
    {/if}
  </div>
</div>

<style>
  .aim-window__body {
    height: 100%;
  }
  p {
    margin: 0;
  }
  .key {
    height: auto;
    padding: 6px;
    font-family: monospace;
    font-size: 13px;
    text-align: center;
    word-spacing: 4px;
    user-select: all;
  }
  .saved {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: auto;
  }
</style>
