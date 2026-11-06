<!-- Verify This Computer: compare emoji with another of your sessions. -->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { watchStep, type Emoji, type Step } from "$lib/verify";

  let step = $state<Step | null>(null);
  // Kept from "compare" so they stay up while waiting for the other side.
  let emojis = $state<Emoji[]>([]);
  let busy = $state(false);
  let error = $state("");

  $effect(() =>
    watchStep((s) => {
      step = s;
      if (s?.step === "compare") emojis = s.emojis;
      busy = false;
    }),
  );

  const active = $derived(step !== null && !["done", "cancelled"].includes(step.step));

  async function run(command: string, args?: Record<string, unknown>) {
    busy = true;
    error = "";
    try {
      // The next step arrives as an event, which clears `busy`.
      await invoke(command, args);
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }

  // Closing the window cancels a verification that's still going (in Rust).
  function close() {
    getCurrentWebviewWindow().close();
  }
</script>

<div class="aim-window__body">
  {#if step === null || step.step === "waiting"}
    <p>Open Element (or another app you're signed on to) and accept the request to verify this computer.</p>
    <p class="aim-msg__note">Waiting for another session…</p>
  {:else if step.step === "incoming"}
    <p><b>{step.device}</b> wants to verify this computer.</p>
    <p>Only accept if you just asked for this from that session.</p>
  {:else if step.step === "starting" && step.canStart}
    <p>Accepted. If <b>{step.device}</b> offers to verify by emoji, choose that there.</p>
    <p>If it's waiting for this computer instead, click Start.</p>
  {:else if step.step === "starting"}
    <p>Getting emoji from <b>{step.device}</b>…</p>
  {:else if step.step === "compare" || step.step === "confirmed"}
    <p>Check that these emoji appear in the same order on <b>{step.device}</b>:</p>
    <ul class="emojis">
      {#each emojis as emoji}
        <li><span class="symbol">{emoji.symbol}</span>{emoji.description}</li>
      {/each}
    </ul>
    {#if step.step === "confirmed"}
      <p class="aim-msg__note">Waiting for {step.device} to confirm…</p>
    {/if}
  {:else if step.step === "done"}
    <p>✔ This computer is verified. Your other sessions now trust it.</p>
  {:else if step.step === "cancelled"}
    <p class="aim-alert" role="alert">{step.reason}</p>
  {/if}

  {#if error}
    <p class="aim-alert" role="alert">{error}</p>
  {/if}

  <div class="actions">
    {#if step?.step === "incoming"}
      <button class="aim-btn" onclick={close} disabled={busy}>Decline</button>
      <button class="aim-btn is-default" onclick={() => run("accept_verification")} disabled={busy}>Accept</button>
    {:else if step?.step === "starting" && step.canStart}
      <button class="aim-btn" onclick={close}>Cancel</button>
      <button class="aim-btn is-default" onclick={() => run("start_verification")} disabled={busy}>Start</button>
    {:else if step?.step === "compare"}
      <button class="aim-btn" onclick={() => run("confirm_verification", { matched: false })} disabled={busy}>
        They Don't Match
      </button>
      <button class="aim-btn is-default" onclick={() => run("confirm_verification", { matched: true })} disabled={busy}>
        They Match
      </button>
    {:else if active}
      <button class="aim-btn" onclick={close}>Cancel</button>
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
  .emojis {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .emojis li {
    display: flex;
    flex-direction: column;
    align-items: center;
    min-width: 56px;
    font-size: 11px;
  }
  .symbol {
    font-size: 28px;
    line-height: 1.2;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: auto;
  }
</style>
