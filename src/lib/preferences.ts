import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `Preferences` in src-tauri/src/preferences.rs. */
export type Preferences = {
  /** While away, answer each conversation's first message with the away message. */
  autoReply: boolean;
  /** The color your messages are sent in, `#rrggbb`. */
  textColor: string;
  /** One of `FONT_FACES`; "" is the app's own font. */
  fontFace: string;
  fontSize: FontSize;
};

export type FontSize = "small" | "normal" | "large" | "larger";

/** Fonts for reading conversations (`FONT_FACES` in preferences.rs), with labels. */
export const FONT_FACES: [string, string][] = [
  ["", "Default"],
  ["Arial", "Arial"],
  ["Comic Sans MS", "Comic Sans MS"],
  ["Courier New", "Courier New"],
  ["Georgia", "Georgia"],
  ["Times New Roman", "Times New Roman"],
  ["Trebuchet MS", "Trebuchet MS"],
  ["Verdana", "Verdana"],
];

/** Text sizes for conversations, with labels and sizes in px. */
export const FONT_SIZES: [FontSize, string, number][] = [
  ["small", "Small", 11],
  ["normal", "Normal", 12],
  ["large", "Large", 14],
  ["larger", "Larger", 16],
];

/**
 * Call `onChange` with the preferences and then every change. Returns a
 * function that stops listening.
 */
export function watchPreferences(onChange: (prefs: Preferences) => void): () => void {
  let stopped = false;
  // Listen before asking, so a change in between isn't missed.
  const unlisten = listen<Preferences>("preferences:update", (e) => onChange(e.payload));
  invoke<Preferences>("get_preferences").then((p) => !stopped && onChange(p));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}

/** Show this window's conversation in the preferred font, via CSS variables. */
export function applyFonts(prefs: Preferences) {
  const root = document.documentElement.style;
  const size = FONT_SIZES.find(([s]) => s === prefs.fontSize)?.[2] ?? 12;
  root.setProperty("--font-size-conversation", `${size}px`);
  if (prefs.fontFace) {
    root.setProperty("--font-conversation", `"${prefs.fontFace}", var(--font-ui)`);
  } else {
    root.removeProperty("--font-conversation");
  }
}
