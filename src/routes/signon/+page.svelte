<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { page } from "$app/state";
  import { tick } from "svelte";

  /** The servers offered, as [address, label]. Dev builds add the local test servers. */
  const SERVERS: [string, string][] = [
    ["matrix.org", "matrix.org"],
    ...(import.meta.env.DEV
      ? ([
          ["http://localhost:8008", "Dev server (localhost:8008)"],
          ["http://localhost:8009", "Dev, no presence (localhost:8009)"],
        ] as [string, string][])
      : []),
  ];
  const CUSTOM = "custom";
  /** Where the last server you signed on to is remembered. Not secret. */
  const REMEMBER = "aimtrix.signon.server";

  /** The last server you used, if this computer remembers one. */
  function remembered(): string | null {
    try {
      return localStorage.getItem(REMEMBER);
    } catch {
      return null;
    }
  }

  const last = remembered();
  const known = SERVERS.some(([address]) => address === last);
  /** A server from the list, or `CUSTOM`. */
  let server = $state(last === null ? SERVERS[0][0] : known ? last : CUSTOM);
  let custom = $state(last !== null && !known ? last : "");
  const homeserver = $derived(server === CUSTOM ? custom.trim() : server);
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
      // Remembered first: on success Rust closes this window.
      try {
        localStorage.setItem(REMEMBER, homeserver);
      } catch {
        // Only a convenience; signing on works without it.
      }
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
        Server
        <select class="aim-field" bind:value={server}>
          {#each SERVERS as [address, label] (address)}
            <option value={address}>{label}</option>
          {/each}
          <option value={CUSTOM}>Custom…</option>
        </select>
      </label>
      {#if server === CUSTOM}
        <label class="aim-label">
          Homeserver
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="aim-field"
            bind:value={custom}
            placeholder="example.com or https://…"
            autocomplete="url"
            spellcheck="false"
            autofocus={custom === ""}
            required
          />
        </label>
      {/if}
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
