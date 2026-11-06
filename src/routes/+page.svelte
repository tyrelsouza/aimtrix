<!-- Buddy List placeholder until step 2.2. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let userId = $state<string | null>(null);
  let busy = $state(false);
  let error = $state("");

  invoke<string | null>("whoami").then((id) => (userId = id));

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

<main class="aim-window__body">
  <p>Signed on as {userId ?? "…"}</p>
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

<style>
  p {
    margin: 0;
  }
</style>
