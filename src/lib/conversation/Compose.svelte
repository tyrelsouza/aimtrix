<!--
  The formatting toolbar, message box and Send button, shared by the IM and
  Chat Room windows. Sends typing notices while you type.
-->
<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { Snippet } from "svelte";
  import { COLORS, serialize } from "$lib/compose";
  import { pasteClipboardPicture, pastedPictures, sendFiles, sendPasted } from "./upload";

  let {
    roomId,
    error = $bindable(""),
    uploading = false,
    beside,
    actions,
  }: {
    roomId: string;
    error?: string;
    /** Files are being sent from this window. */
    uploading?: boolean;
    /** Shown to the right of the message box, e.g. your Buddy Icon. */
    beside?: Snippet;
    /** More buttons, before Send File…, e.g. Block. */
    actions?: Snippet;
  } = $props();

  /** The editor's plain text, to know whether there's anything to send. */
  let draft = $state("");
  let editor: HTMLDivElement;
  /** Toolbar state at the cursor. */
  let bold = $state(false);
  let italic = $state(false);
  let underline = $state(false);
  let color = $state(COLORS[0][0]);
  let paletteOpen = $state(false);

  // Ready to type when the window opens.
  $effect(() => editor.focus());

  // Your own typing notice: started when you type, renewed while you keep
  // typing, and stopped when you send, clear the box or pause. The server
  // drops it after a few seconds if it isn't renewed.
  const RENEW_EVERY = 2000;
  const IDLE_AFTER = 5000;
  let typingSent = false;
  let typingSentAt = 0;
  let idleTimer: ReturnType<typeof setTimeout> | undefined;

  function typed() {
    clearTimeout(idleTimer);
    if (!draft.trim()) {
      stopTyping();
      return;
    }
    if (!typingSent || Date.now() - typingSentAt > RENEW_EVERY) {
      typingSent = true;
      typingSentAt = Date.now();
      setTyping(true);
    }
    // Stopped typing without sending: say so after a while.
    idleTimer = setTimeout(stopTyping, IDLE_AFTER);
  }

  function stopTyping() {
    clearTimeout(idleTimer);
    if (typingSent) {
      typingSent = false;
      setTyping(false);
    }
  }

  function setTyping(on: boolean) {
    // A lost typing notice isn't worth interrupting you for.
    invoke("set_typing", { roomId, typing: on }).catch((e) =>
      console.warn("Couldn't send the typing notice:", e),
    );
  }

  /** Enter sends; Shift+Enter and Alt+Enter start a new line. */
  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape" && paletteOpen) {
      paletteOpen = false;
      return;
    }
    if (e.key !== "Enter" || e.isComposing) return;
    e.preventDefault();
    if (e.shiftKey || e.altKey) {
      document.execCommand("insertLineBreak");
    } else {
      send();
    }
  }

  function edited() {
    draft = serialize(editor).body;
    typed();
  }

  /**
   * Paste a picture to send it (Rust asks first), and text as plain text,
   * so outside styles don't come along.
   */
  function paste(e: ClipboardEvent) {
    e.preventDefault();
    const text = e.clipboardData?.getData("text/plain") ?? "";
    if (text) {
      document.execCommand("insertText", false, text);
      return;
    }
    if (uploading) {
      error = "Wait for the file being sent to finish, or cancel it.";
      return;
    }
    error = "";
    // A paste holds one picture in practice; a window sends one at a time.
    // WebKitGTK doesn't hand pasted pictures over, so Rust reads the
    // clipboard itself (and does nothing if there's no picture).
    const pictures = pastedPictures(e);
    const sending = pictures.length > 0 ? sendPasted(roomId, pictures[0]) : pasteClipboardPicture(roomId);
    sending.catch((err) => (error = String(err)));
  }

  /** Keep the toolbar showing the formatting at the cursor. */
  function selectionChanged() {
    if (!editor?.contains(document.getSelection()?.anchorNode ?? null)) return;
    bold = document.queryCommandState("bold");
    italic = document.queryCommandState("italic");
    underline = document.queryCommandState("underline");
  }

  function format(command: "bold" | "italic" | "underline") {
    editor.focus();
    document.execCommand(command);
    selectionChanged();
    edited();
  }

  function pickColor(hex: string) {
    paletteOpen = false;
    color = hex;
    editor.focus();
    document.execCommand("foreColor", false, hex);
    edited();
  }

  async function pickFiles() {
    error = "";
    try {
      await sendFiles(roomId);
    } catch (err) {
      error = String(err);
    }
  }

  async function send(e?: SubmitEvent) {
    e?.preventDefault();
    const { body, html } = serialize(editor);
    if (!body.trim()) return;
    const saved = editor.innerHTML;
    // Clear at once: the message shows up in the transcript straight away.
    editor.innerHTML = "";
    draft = "";
    stopTyping();
    error = "";
    editor.focus();
    try {
      await invoke("send_message", { roomId, body, html });
    } catch (err) {
      error = String(err);
      // Give the text back to retry, unless something new was typed.
      if (!draft) {
        editor.innerHTML = saved;
        draft = body;
      }
    }
  }
</script>

<svelte:document onselectionchange={selectionChanged} />

<form class="compose" onsubmit={send}>
  <!-- Buttons keep the editor's selection: mousedown would take focus. -->
  <div class="aim-toolbar" role="toolbar" aria-label="Formatting">
    <button type="button" class="aim-toolbar__btn" title="Bold" aria-pressed={bold}
      onmousedown={(e) => e.preventDefault()} onclick={() => format("bold")}><b>B</b></button>
    <button type="button" class="aim-toolbar__btn" title="Italic" aria-pressed={italic}
      onmousedown={(e) => e.preventDefault()} onclick={() => format("italic")}><i>I</i></button>
    <button type="button" class="aim-toolbar__btn" title="Underline" aria-pressed={underline}
      onmousedown={(e) => e.preventDefault()} onclick={() => format("underline")}><u>U</u></button>
    <span class="aim-toolbar__sep"></span>
    <div class="color">
      <button type="button" class="aim-toolbar__btn" title="Text color" aria-haspopup="true"
        aria-expanded={paletteOpen} style:color={color}
        onmousedown={(e) => e.preventDefault()} onclick={() => (paletteOpen = !paletteOpen)}>A</button>
      {#if paletteOpen}
        <div class="aim-palette" role="menu" aria-label="Text color">
          {#each COLORS as [hex, label] (hex)}
            <button type="button" class="aim-palette__swatch" role="menuitem" title={label}
              aria-label={label} style:background={hex}
              onmousedown={(e) => e.preventDefault()} onclick={() => pickColor(hex)}></button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
  <div class="row">
    <div
      class="aim-field editor"
      contenteditable="true"
      role="textbox"
      tabindex="0"
      aria-multiline="true"
      aria-label="Message"
      bind:this={editor}
      onkeydown={keydown}
      oninput={edited}
      onpaste={paste}
      ondrop={(e) => e.preventDefault()}
    ></div>
    {@render beside?.()}
  </div>
  <div class="buttons">
    <span class="left">
      {@render actions?.()}
      <button type="button" class="aim-btn" disabled={uploading} onclick={pickFiles}
        title="Send a file or picture. You can also drop files on this window, or paste a picture.">Send File…</button>
    </span>
    <button class="aim-btn is-default" disabled={!draft.trim()}>Send</button>
  </div>
</form>

<style>
  .compose {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .editor {
    flex: 1;
    min-width: 0;
    /* About three lines, then it scrolls. */
    height: 4.6em;
    overflow-y: auto;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .color {
    position: relative;
  }
  .buttons {
    display: flex;
    justify-content: space-between;
  }
  .left {
    display: flex;
    gap: 6px;
  }
</style>
