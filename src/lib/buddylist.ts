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
  /** The other person's presence, for a joined DM. Null when the server has no presence. */
  presence: Presence | null;
};

export type Presence = "online" | "away" | "offline";

/** "buddies" holds DMs when the server has no presence; otherwise they're in "online" and "offline". */
export type Section = "favorites" | "buddies" | "online" | "chats" | "invitations" | "offline";

/** Buddy List groups in display order, with their titles. */
export const SECTIONS: [Section, string][] = [
  ["favorites", "Favorites"],
  ["buddies", "Buddies"],
  ["online", "Online"],
  ["chats", "Chats"],
  ["invitations", "Invitations"],
  ["offline", "Offline"],
];

/**
 * The groups to show: Online and Offline when the server has presence,
 * Buddies when it doesn't, and Invitations only when there are some.
 */
export function visibleSections(buddies: Buddy[]): [Section, string][] {
  const presence = buddies.some((b) => b.presence !== null);
  const hidden = new Set<Section>(presence ? ["buddies"] : ["online", "offline"]);
  if (!buddies.some((b) => b.section === "invitations")) hidden.add("invitations");
  return SECTIONS.filter(([section]) => !hidden.has(section));
}

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
