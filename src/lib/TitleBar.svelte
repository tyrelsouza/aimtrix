<!--
  The AIM-style title bar every window has instead of the platform's
  (windows are created without decorations, `windows::aim_window`): drag it
  to move the window, double-click to maximize (resizable windows only), and
  minimize, maximize and close buttons.
-->
<script lang="ts">
  import { getCurrentWindow } from "@tauri-apps/api/window";

  const win = getCurrentWindow();
  let title = $state("");
  let active = $state(true);
  let resizable = $state(false);

  win.title().then((t) => (title = t));
  win.isFocused().then((f) => (active = f));
  win.isResizable().then((r) => (resizable = r));

  $effect(() => {
    const unlisten = win.onFocusChanged(({ payload }) => (active = payload));
    return () => {
      unlisten.then((stop) => stop());
    };
  });

  function grab(event: MouseEvent) {
    // Only the bar itself, not its buttons, and only the left button.
    if (event.button !== 0 || (event.target as Element).closest("button")) return;
    if (event.detail === 2) {
      if (resizable) win.toggleMaximize();
    } else {
      win.startDragging();
    }
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="aim-titlebar" class:is-inactive={!active} onmousedown={grab}>
  <img class="aim-titlebar__icon" src="/favicon.png" alt="" draggable="false" />
  <span class="aim-titlebar__title">{title}</span>
  <div class="aim-titlebar__controls">
    <button
      class="aim-titlebar__btn aim-titlebar__btn--minimize"
      aria-label="Minimize"
      tabindex="-1"
      onclick={() => win.minimize()}
    ></button>
    {#if resizable}
      <button
        class="aim-titlebar__btn aim-titlebar__btn--maximize"
        aria-label="Maximize"
        tabindex="-1"
        onclick={() => win.toggleMaximize()}
      ></button>
    {/if}
    <button
      class="aim-titlebar__btn aim-titlebar__btn--close"
      aria-label="Close"
      tabindex="-1"
      onclick={() => win.close()}
    ></button>
  </div>
</div>
