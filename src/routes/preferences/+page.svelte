<!-- Preferences: auto-reply and fonts. Changes apply when you click OK. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { COLORS } from "$lib/compose";
  import { FONT_FACES, FONT_SIZES, type Preferences } from "$lib/preferences";

  let prefs = $state<Preferences | null>(null);
  let busy = $state(false);
  let error = $state("");

  invoke<Preferences>("get_preferences")
    .then((p) => (prefs = p))
    .catch((e) => (error = String(e)));

  const sizePx = $derived(FONT_SIZES.find(([s]) => s === prefs?.fontSize)?.[2] ?? 12);

  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (!prefs || busy) return;
    busy = true;
    error = "";
    try {
      await invoke("set_preferences", { preferences: prefs });
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

<form class="aim-window__body" onsubmit={save}>
  {#if prefs}
    <fieldset class="aim-groupbox" disabled={busy}>
      <legend>Away</legend>
      <label class="check">
        <input type="checkbox" bind:checked={prefs.autoReply} />
        Send my away message as an auto-reply
      </label>
      <p class="hint">Once per conversation, to the first message that arrives while you're away.</p>
    </fieldset>

    <fieldset class="aim-groupbox" disabled={busy}>
      <legend>Fonts</legend>
      <div class="grid">
        <label for="face">Conversations:</label>
        <select id="face" class="aim-field" bind:value={prefs.fontFace}>
          {#each FONT_FACES as [face, label] (face)}
            <option value={face}>{label}</option>
          {/each}
        </select>
        <label for="size">Size:</label>
        <select id="size" class="aim-field" bind:value={prefs.fontSize}>
          {#each FONT_SIZES as [size, label] (size)}
            <option value={size}>{label}</option>
          {/each}
        </select>
        <span>My messages:</span>
        <div class="swatches" role="radiogroup" aria-label="Color of my messages">
          {#each COLORS as [hex, label] (hex)}
            <button
              type="button"
              class="aim-palette__swatch"
              class:is-chosen={prefs.textColor === hex}
              role="radio"
              aria-checked={prefs.textColor === hex}
              aria-label={label}
              title={label}
              style:background={hex}
              onclick={() => prefs && (prefs.textColor = hex)}
            ></button>
          {/each}
        </div>
      </div>
      <p
        class="aim-field sample"
        style:font-family={prefs.fontFace ? `"${prefs.fontFace}", var(--font-ui)` : undefined}
        style:font-size="{sizePx}px"
      >
        <span style:color={prefs.textColor}>Me: Hey, what's up?</span>
      </p>
    </fieldset>
  {:else if !error}
    <p role="status">Loading…</p>
  {/if}

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {/if}

  <div class="actions">
    <button class="aim-btn" type="button" onclick={close}>Cancel</button>
    <button class="aim-btn is-default" type="submit" disabled={busy || !prefs}>OK</button>
  </div>
</form>

<style>
  .aim-window__body {
    height: 100%;
  }
  p {
    margin: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .hint {
    color: var(--c-text-disabled);
    margin-left: 20px;
  }
  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 4px 6px;
  }
  .swatches {
    display: flex;
    gap: 2px;
  }
  .is-chosen {
    outline: 2px solid var(--c-select);
    outline-offset: 1px;
  }
  .sample {
    margin-top: 6px;
    overflow: hidden;
    white-space: nowrap;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: auto;
  }
</style>
