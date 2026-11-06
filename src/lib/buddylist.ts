import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Mirrors `Buddy` in src-tauri/src/matrix/rooms.rs. */
export type Buddy = {
  roomId: string;
  name: string;
  kind: "dm" | "group";
  section: Section;
  unread: boolean;
  /** `data:` URL, or null for the default icon. */
  icon: string | null;
};

export type Section = "favorites" | "buddies" | "chats" | "invitations";

/** Buddy List groups in display order, with their titles. */
export const SECTIONS: [Section, string][] = [
  ["favorites", "Favorites"],
  ["buddies", "Buddies"],
  ["chats", "Chats"],
  ["invitations", "Invitations"],
];

/**
 * Call `onChange` with the current Buddy List and then every change.
 * Returns a function that stops listening.
 */
export function watchBuddyList(onChange: (buddies: Buddy[]) => void): () => void {
  let stopped = false;
  // Listen before asking, so a change in between isn't missed.
  const unlisten = listen<Buddy[]>("buddylist:update", (e) => onChange(e.payload));
  invoke<Buddy[]>("buddy_list").then((b) => !stopped && onChange(b));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}
