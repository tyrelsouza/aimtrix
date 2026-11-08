import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * Follow a value the Rust core keeps: call `onChange` with its current value
 * (from `command`) and then with every change (from `event`, which always
 * carries the whole new value). Returns a function that stops listening.
 *
 * It listens before asking, so a change in between isn't missed, and an
 * event that arrives before the answer wins, since it's newer.
 */
export function watchValue<T>(event: string, command: string, onChange: (value: T) => void): () => void {
  let stopped = false;
  let changed = false;
  const unlisten = listen<T>(event, (e) => {
    if (stopped) return;
    changed = true;
    onChange(e.payload);
  });
  unlisten
    .then(() => invoke<T>(command))
    .then((value) => !stopped && !changed && onChange(value))
    .catch((e) => console.error(`Couldn't follow ${event}:`, e));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}
