import { watchValue } from "$lib/watch";

/** Mirrors `Recovery` in src-tauri/src/matrix/crypto.rs. */
export type Recovery = "unknown" | "enabled" | "disabled" | "incomplete";

/**
 * Call `onChange` with whether this device can read old messages, and then
 * every change. Returns a function that stops listening.
 */
export function watchRecovery(onChange: (recovery: Recovery) => void): () => void {
  return watchValue("recovery:status", "recovery_status", onChange);
}
