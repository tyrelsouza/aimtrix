<!-- About AIMtrix: version, license, and the licenses of what it's built with. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

  let version = $state("");
  let error = $state("");
  getVersion().then((v) => (version = v));

  async function open(which: "license" | "third-party") {
    error = "";
    try {
      await invoke("open_license", { which });
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="aim-window__body">
  <div class="title">
    <img src="/favicon.png" alt="" width="48" height="48" />
    <div>
      <p class="name">AIMtrix</p>
      <p>Version {version}</p>
    </div>
  </div>
  <p>A Matrix chat client in the style of AOL Instant Messenger. Not affiliated with AOL.</p>
  <p>© 2026 Tyrel Souza. MIT License.</p>
  <p>Built with open-source software under its own licenses.</p>

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {/if}

  <div class="actions">
    <button class="aim-btn" type="button" onclick={() => open("license")}>License…</button>
    <button class="aim-btn" type="button" onclick={() => open("third-party")}>Third-Party…</button>
    <button class="aim-btn is-default" type="button" onclick={() => getCurrentWebviewWindow().close()}
      >OK</button
    >
  </div>
</div>

<style>
  .aim-window__body {
    height: 100%;
    gap: 6px;
  }
  p {
    margin: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .name {
    font-weight: bold;
    font-size: 16px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: auto;
  }
</style>
