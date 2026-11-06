import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Mirrors `Line` in src-tauri/src/matrix/timeline.rs. */
export type Line =
  | {
      type: "message";
      id: string;
      sender: string;
      name: string;
      own: boolean;
      body: string;
      emote: boolean;
      /** Ours, and the server didn't take it. */
      failed: boolean;
      /** Milliseconds since the Unix epoch. */
      timestamp: number;
    }
  /** The room's first event is loaded: there's no more history. */
  | { type: "start"; id: string }
  | { type: "other"; id: string };

/** Mirrors `Diff` in src-tauri/src/matrix/timeline.rs. */
type Diff =
  | { op: "append"; values: Line[] }
  | { op: "clear" }
  | { op: "pushFront"; value: Line }
  | { op: "pushBack"; value: Line }
  | { op: "popFront" }
  | { op: "popBack" }
  | { op: "insert"; index: number; value: Line }
  | { op: "set"; index: number; value: Line }
  | { op: "remove"; index: number }
  | { op: "truncate"; length: number }
  | { op: "reset"; values: Line[] };

function apply(lines: Line[], diff: Diff): Line[] {
  switch (diff.op) {
    case "append":
      return [...lines, ...diff.values];
    case "clear":
      return [];
    case "pushFront":
      return [diff.value, ...lines];
    case "pushBack":
      return [...lines, diff.value];
    case "popFront":
      return lines.slice(1);
    case "popBack":
      return lines.slice(0, -1);
    case "insert":
      return lines.toSpliced(diff.index, 0, diff.value);
    case "set":
      return lines.with(diff.index, diff.value);
    case "remove":
      return lines.toSpliced(diff.index, 1);
    case "truncate":
      return lines.slice(0, diff.length);
    case "reset":
      return diff.values;
  }
}

/**
 * Call `onChange` with this room window's conversation and then after every
 * change. Returns a function that stops listening.
 */
export function watchTimeline(
  roomId: string,
  onChange: (lines: Line[]) => void,
  onError: (message: string) => void,
): () => void {
  // The window's label is `room-<key>`; its events are `room:<key>:…`.
  const key = getCurrentWebviewWindow().label.replace(/^room-/, "");
  let stopped = false;
  let lines: Line[] | null = null;
  // Diffs that arrive before the first lines come after them, so hold them.
  const early: Diff[] = [];

  const unlisten = listen<Diff>(`room:${key}:timeline_diff`, (e) => {
    if (stopped) return;
    if (lines === null) {
      early.push(e.payload);
    } else {
      lines = apply(lines, e.payload);
      onChange(lines);
    }
  });
  // Listen first, so no change between the two is missed.
  unlisten
    .then(() => invoke<Line[]>("room_timeline", { roomId }))
    .then((first) => {
      if (stopped) return;
      lines = early.reduce(apply, first);
      onChange(lines);
    })
    .catch((e) => !stopped && onError(String(e)));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}
