import { watchValue } from "$lib/watch";

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
  /** The other person's status message, shown as their away message. */
  awayMessage: string | null;
  /** The other person, for a DM. */
  userId: string | null;
  /** The other person in a DM is blocked. */
  blocked: boolean;
};

/** Hover text for a buddy: their presence and away message. */
export function buddyTooltip(buddy: Buddy): string | undefined {
  if (buddy.blocked) return `${buddy.name} is blocked.`;
  switch (buddy.presence) {
    case "away":
      return buddy.awayMessage ? `${buddy.name} is away:\n${buddy.awayMessage}` : `${buddy.name} is away.`;
    case "offline":
      return `${buddy.name} is offline.`;
    case "online":
      return buddy.awayMessage ?? undefined;
    default:
      return undefined;
  }
}

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
 * The groups to show: Online and Offline when the server has presence
 * (`serverHasPresence`, from `own_presence`),
 * Buddies when it doesn't, and Invitations only when there are some.
 */
export function visibleSections(buddies: Buddy[], serverHasPresence: boolean | null): [Section, string][] {
  // Until AIMtrix knows (null), go by whether any buddy has a presence.
  const presence = serverHasPresence ?? buddies.some((b) => b.presence !== null);
  const hidden = new Set<Section>(presence ? ["buddies"] : ["online", "offline"]);
  if (!buddies.some((b) => b.section === "invitations")) hidden.add("invitations");
  return SECTIONS.filter(([section]) => !hidden.has(section));
}

/**
 * Call `onChange` with the current Buddy List and then every change.
 * Returns a function that stops listening.
 */
export function watchBuddyList(onChange: (buddies: Buddy[]) => void): () => void {
  return watchValue("buddylist:update", "buddy_list", onChange);
}
