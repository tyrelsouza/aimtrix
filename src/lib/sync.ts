import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `Status` in src-tauri/src/matrix/sync.rs. */
export type SyncStatus =
  | { state: "connecting" }
  | { state: "online" }
  | { state: "offline" }
  | { state: "error"; message: string }
  | { state: "signed-out" };

export function describe(status: SyncStatus): string {
  switch (status.state) {
    case "connecting":
      return "Connecting…";
    case "online":
      return "Online";
    case "offline":
      return "Offline. Reconnecting…";
    case "error":
      return "Connection problem. Retrying…";
    case "signed-out":
      return "Signed out";
  }
}

/**
 * Call `onChange` with the current status and then every change.
 * Returns a function that stops listening.
 */
export function watchSyncStatus(onChange: (status: SyncStatus) => void): () => void {
  let stopped = false;
  // Listen before asking, so a change in between isn't missed.
  const unlisten = listen<SyncStatus>("sync:status", (e) => onChange(e.payload));
  invoke<SyncStatus>("sync_status").then((s) => !stopped && onChange(s));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}
