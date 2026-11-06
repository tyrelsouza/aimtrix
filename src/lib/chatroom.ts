import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { roomKey } from "$lib/timeline";

/** Mirrors `Member` in src-tauri/src/matrix/members.rs. */
export type Member = {
  userId: string;
  name: string;
  /** You. */
  own: boolean;
  /** Buddy Icon as a `data:` URL; null means the default icon. */
  icon: string | null;
};

/** Mirrors `ChatRoom` in src-tauri/src/matrix/members.rs. */
type ChatRoom = { members: Member[]; topic: string | null };

/**
 * Follow who's in this Chat Room window's room, sorted by name, and its
 * topic (null if none). Each callback gets the current value, then every
 * change. Returns a function that stops listening.
 */
export function watchChatRoom(
  roomId: string,
  onMembers: (members: Member[]) => void,
  onTopic: (topic: string | null) => void,
  onError: (message: string) => void,
): () => void {
  const key = roomKey();
  let stopped = false;
  // Events are always the latest; one that beats the first answer is newer.
  let membersUpdated = false;
  let topicUpdated = false;
  const unlisten = Promise.all([
    listen<Member[]>(`room:${key}:members`, (e) => {
      if (stopped) return;
      membersUpdated = true;
      onMembers(e.payload);
    }),
    listen<string | null>(`room:${key}:topic`, (e) => {
      if (stopped) return;
      topicUpdated = true;
      onTopic(e.payload);
    }),
  ]);
  // Listen first, so no change between the two is missed.
  unlisten
    .then(() => invoke<ChatRoom>("chat_room", { roomId }))
    .then((first) => {
      if (stopped) return;
      if (!membersUpdated) onMembers(first.members);
      if (!topicUpdated) onTopic(first.topic);
    })
    .catch((e) => !stopped && onError(String(e)));
  return () => {
    stopped = true;
    unlisten.then((fs) => fs.forEach((f) => f()));
  };
}

/** "3 people here". */
export function describeCount(count: number): string {
  return count === 1 ? "1 person here" : `${count} people here`;
}
