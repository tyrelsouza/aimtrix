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
          ["http://localhost:8011", "Dev, continuwuity (localhost:8011)"],
          ["http://localhost:8013", "Dev, behind MAS (localhost:8013)"],
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
  let passwordInput: HTMLInputElement | undefined = $state();

  /** How the chosen server lets people sign on (`sign_on_methods`). */
  type Methods = { password: boolean; browser: boolean };
  /** Until the server says otherwise, offer the password. */
  let methods: Methods = $state({ password: true, browser: false });
  /** Waiting for the browser to come back from signing on. */
  let inBrowser = $state(false);

  // Ask the server how it lets people sign on, after typing pauses.
  $effect(() => {
    const address = homeserver;
    methods = { password: true, browser: false };
    if (address === "") return;
    let stale = false;
    const timer = setTimeout(
      async () => {
        try {
          const found = await invoke<Methods>("sign_on_methods", { homeserver: address });
          if (!stale) methods = found;
        } catch {
          // Unreachable or not a server yet; Sign On will say why.
        }
      },
      server === CUSTOM ? 600 : 0,
    );
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  });

  function remember() {
    // Remembered first: on success Rust closes this window.
    try {
      localStorage.setItem(REMEMBER, homeserver);
    } catch {
      // Only a convenience; signing on works without it.
    }
  }

  async function signOnWithBrowser() {
    busy = true;
    inBrowser = true;
    error = "";
    remember();
    try {
      // Rust opens the browser and waits for it; on success it opens the
      // Buddy List and closes this window.
      await invoke("login_with_browser", { homeserver });
    } catch (e) {
      error = String(e);
      busy = false;
      inBrowser = false;
    }
  }

  function cancelBrowser() {
    invoke("cancel_browser_login");
  }

  async function signOn(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = "";
    remember();
    try {
      // On success Rust opens the Buddy List and closes this window.
      await invoke("login", { homeserver, username, password });
    } catch (e) {
      error = String(e);
      busy = false;
      // The disabled fieldset dropped focus; put it back so you can just retype.
      await tick();
      passwordInput?.select();
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
      {#if methods.password}
        <label class="aim-label">
          Screen Name
          <!-- svelte-ignore a11y_autofocus -->
          <input class="aim-field" bind:value={username} autocomplete="username" autofocus required />
        </label>
        <label class="aim-label">
          Password
          <input class="aim-field" type="password" bind:this={passwordInput} bind:value={password} autocomplete="current-password" required />
        </label>
      {:else}
        <p>This server signs you on in your web browser.</p>
      {/if}
    </div>
  </fieldset>

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {:else if inBrowser}
    <p role="status">Finish signing on in your web browser…</p>
  {:else if busy}
    <p role="status">Signing on…</p>
  {/if}

  <div class="actions">
    {#if inBrowser}
      <button class="aim-btn" type="button" onclick={cancelBrowser}>Cancel</button>
    {:else}
      {#if methods.browser}
        <button
          class="aim-btn"
          class:is-default={!methods.password}
          type="button"
          disabled={busy}
          onclick={signOnWithBrowser}
          title="For accounts without a password, such as ones made with Google, GitHub or Apple"
          >{methods.password ? "Use Browser…" : "Sign On with Browser…"}</button
        >
      {/if}
      {#if methods.password}
        <button class="aim-btn is-default" type="submit" disabled={busy}>Sign On</button>
      {/if}
    {/if}
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
    gap: 6px;
    margin-top: auto;
  }
</style>
