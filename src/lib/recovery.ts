import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `Recovery` in src-tauri/src/matrix/crypto.rs. */
export type Recovery = "unknown" | "enabled" | "disabled" | "incomplete";

/**
 * Call `onChange` with whether this device can read old messages, and then
 * every change. Returns a function that stops listening.
 */
export function watchRecovery(onChange: (recovery: Recovery) => void): () => void {
  let stopped = false;
  // Listen before asking, so a change in between isn't missed.
  const unlisten = listen<Recovery>("recovery:status", (e) => onChange(e.payload));
  invoke<Recovery>("recovery_status").then((r) => !stopped && onChange(r));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}
