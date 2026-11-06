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

/**
 * Call `onChange` with who's in this room window's room, sorted by name,
 * and again after every change. Returns a function that stops listening.
 */
export function watchMembers(
  roomId: string,
  onChange: (members: Member[]) => void,
  onError: (message: string) => void,
): () => void {
  let stopped = false;
  // Each event is the whole list, newer than the first one if it came first.
  let updated = false;
  const unlisten = listen<Member[]>(`room:${roomKey()}:members`, (e) => {
    if (stopped) return;
    updated = true;
    onChange(e.payload);
  });
  // Listen first, so no change between the two is missed.
  unlisten
    .then(() => invoke<Member[]>("room_members", { roomId }))
    .then((first) => !stopped && !updated && onChange(first))
    .catch((e) => !stopped && onError(String(e)));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}

/** "3 people here". */
export function describeCount(count: number): string {
  return count === 1 ? "1 person here" : `${count} people here`;
}
