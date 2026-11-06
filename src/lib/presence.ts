import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `Own` in src-tauri/src/matrix/presence.rs. */
export type OwnPresence = {
  /** Whether the server has presence. Null until AIMtrix knows. */
  supported: boolean | null;
  /** Your away message while you're away. */
  away: string | null;
};

/** What a new away message starts as. */
export const DEFAULT_AWAY_MESSAGE = "I am away from my computer right now.";

/** Longest away message, in characters (`MAX_AWAY_MESSAGE` in presence.rs). */
export const MAX_AWAY_MESSAGE = 1024;

/**
 * Call `onChange` with your presence and then every change. Returns a
 * function that stops listening.
 */
export function watchOwnPresence(onChange: (own: OwnPresence) => void): () => void {
  let stopped = false;
  // Listen before asking, so a change in between isn't missed.
  const unlisten = listen<OwnPresence>("presence:update", (e) => onChange(e.payload));
  invoke<OwnPresence>("own_presence").then((p) => !stopped && onChange(p));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}
