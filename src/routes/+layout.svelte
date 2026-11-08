<script lang="ts">
  import "../styles/aim-chrome.css";
  import TitleBar from "$lib/TitleBar.svelte";

  let { children } = $props();
</script>

<!-- Every window draws its own AIM-style frame and title bar (PLAN.md 8.4). -->
<div class="aim-window aim-frame">
  <TitleBar />
  <div class="aim-frame__content">
    {@render children()}
  </div>
</div>
