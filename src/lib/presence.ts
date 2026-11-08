import { watchValue } from "$lib/watch";

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
  return watchValue("presence:update", "own_presence", onChange);
}
