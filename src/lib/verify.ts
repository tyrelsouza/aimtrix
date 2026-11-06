import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `Trust` in src-tauri/src/matrix/verify.rs. */
export type Trust = "unknown" | "no-cross-signing" | "verified" | "unverified";

/** Mirrors `Emoji` in src-tauri/src/matrix/verify.rs. */
export type Emoji = { symbol: string; description: string };

/** Mirrors `Step` in src-tauri/src/matrix/verify.rs. `device` is the other session's name. */
export type Step =
  | { step: "waiting" }
  | { step: "incoming"; device: string }
  /** `canStart`: the other session asked, and may wait for us to start comparing. */
  | { step: "starting"; device: string; canStart: boolean }
  | { step: "compare"; device: string; emojis: Emoji[] }
  | { step: "confirmed"; device: string }
  | { step: "done"; device: string }
  | { step: "cancelled"; reason: string };

/**
 * Call `onChange` with whether this device is verified, and then every
 * change. Returns a function that stops listening.
 */
export function watchTrust(onChange: (trust: Trust) => void): () => void {
  let stopped = false;
  // Listen before asking, so a change in between isn't missed.
  const unlisten = listen<Trust>("verification:trust", (e) => onChange(e.payload));
  invoke<Trust>("device_trust").then((t) => !stopped && onChange(t));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}

/**
 * Call `onChange` with the verification's step (null before one starts),
 * and then every change. Returns a function that stops listening.
 */
export function watchStep(onChange: (step: Step | null) => void): () => void {
  let stopped = false;
  let changed = false;
  const unlisten = listen<Step>("verification:step", (e) => {
    changed = true;
    onChange(e.payload);
  });
  // An event that arrived first is newer than this answer.
  invoke<Step | null>("verification_step").then((s) => !stopped && !changed && onChange(s));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}
