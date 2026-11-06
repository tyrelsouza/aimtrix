<!-- Buddy List placeholder until step 2.2. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  let userId = $state<string | null>(null);
  invoke<string | null>("whoami").then((id) => (userId = id));
</script>

<main class="aim-window__body">
  <p>Signed on as {userId ?? "…"}</p>
  {#if import.meta.env.DEV}
    <p><a href="/demo">Style demo</a></p>
  {/if}
</main>
