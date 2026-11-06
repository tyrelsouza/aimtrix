<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { page } from "$app/state";
  import { tick } from "svelte";

  // The dev homeserver from dev/docker-compose.yml, so testing is one less field.
  let homeserver = $state(import.meta.env.DEV ? "http://localhost:8008" : "");
  let username = $state("");
  let password = $state("");
  let busy = $state(false);
  // Set when the server ended the session (e.g. signed out from another client).
  let error = $state(
    page.url.searchParams.has("signed-out") ? "You were signed out. Please sign on again." : "",
  );
  let passwordInput: HTMLInputElement;

  async function signOn(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = "";
    try {
      // On success Rust opens the Buddy List and closes this window.
      await invoke("login", { homeserver, username, password });
    } catch (e) {
      error = String(e);
      busy = false;
      // The disabled fieldset dropped focus; put it back so you can just retype.
      await tick();
      passwordInput.select();
    }
  }
</script>

<form class="aim-window__body" onsubmit={signOn}>
  <fieldset class="aim-groupbox" disabled={busy}>
    <legend>Matrix account</legend>
    <div class="fields">
      <label class="aim-label">
        Homeserver
        <input class="aim-field" bind:value={homeserver} placeholder="matrix.org" required />
      </label>
      <label class="aim-label">
        Screen Name
        <!-- svelte-ignore a11y_autofocus -->
        <input class="aim-field" bind:value={username} autocomplete="username" autofocus required />
      </label>
      <label class="aim-label">
        Password
        <input class="aim-field" type="password" bind:this={passwordInput} bind:value={password} autocomplete="current-password" required />
      </label>
    </div>
  </fieldset>

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {:else if busy}
    <p role="status">Signing on…</p>
  {/if}

  <div class="actions">
    <button class="aim-btn is-default" type="submit" disabled={busy}>Sign On</button>
  </div>
</form>

<style>
  form {
    height: 100%;
  }
  .fields {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  p {
    margin: 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    margin-top: auto;
  }
</style>
