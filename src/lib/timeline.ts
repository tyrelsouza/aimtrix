import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** Mirrors `Line` in src-tauri/src/matrix/timeline.rs. */
export type Line =
  | {
      type: "message";
      id: string;
      /** Null until the server has it (yours, while sending). */
      eventId: string | null;
      sender: string;
      name: string;
      own: boolean;
      /** Plain text, always there. */
      body: string;
      /** Formatted text, already cleaned by the Rust core, or null. */
      html: string | null;
      emote: boolean;
      /** Changed after sending; `body` and `html` are the latest version. */
      edited: boolean;
      /** An image or file; `body` is then its caption, or its name. */
      media: Media | null;
      /** The message this one answers. */
      reply: Reply | null;
      /** Ours, and the server didn't take it. */
      failed: boolean;
      /** Milliseconds since the Unix epoch. */
      timestamp: number;
    }
  /** A deleted message. */
  | { type: "deleted"; id: string; name: string; own: boolean; timestamp: number }
  /** An encrypted message that can't be read yet. Turns into a "message" by itself if the key arrives. */
  | {
      type: "undecryptable";
      id: string;
      eventId: string | null;
      name: string;
      own: boolean;
      reason: UndecryptableReason;
      timestamp: number;
    }
  /** Someone entered or left the room, or was invited, removed or banned. */
  | { type: "membership"; id: string; name: string; change: MemberChange; timestamp: number }
  /** `name` set the topic, or removed it if `topic` is null. */
  | { type: "topic"; id: string; name: string; topic: string | null; timestamp: number }
  /** The room's first event is loaded: there's no more history. */
  | { type: "start"; id: string }
  | { type: "other"; id: string };

/** Mirrors `UndecryptableReason` in src-tauri/src/matrix/timeline.rs. */
export type UndecryptableReason = "waiting" | "before-sign-on" | "before-joined" | "withheld" | "untrusted";

/** The placeholder shown instead of a message that can't be decrypted. */
export function describeUndecryptable(reason: UndecryptableReason): string {
  switch (reason) {
    case "waiting":
      return "(encrypted message, waiting for the key to read it)";
    case "before-sign-on":
      return "(encrypted message sent before this computer signed on)";
    case "before-joined":
      return "(encrypted message sent before you joined)";
    case "withheld":
      return "(encrypted message: the sender didn't share the key)";
    case "untrusted":
      return "(encrypted message from an unverified device)";
  }
}

/** Mirrors `MemberChange` in src-tauri/src/matrix/timeline.rs. */
export type MemberChange = "joined" | "left" | "invited" | "kicked" | "banned";

/** "Bob Brown has entered the room." */
export function describeMemberChange(name: string, change: MemberChange): string {
  switch (change) {
    case "joined":
      return `${name} has entered the room.`;
    case "left":
      return `${name} has left the room.`;
    case "invited":
      return `${name} was invited.`;
    case "kicked":
      return `${name} was removed from the room.`;
    case "banned":
      return `${name} was banned from the room.`;
  }
}

/** Mirrors `Media` in src-tauri/src/matrix/timeline.rs. */
export type Media = {
  kind: "image" | "video" | "audio" | "file";
  name: string;
  /** Bytes, if the sender said. */
  size: number | null;
  width: number | null;
  height: number | null;
};

/** Mirrors `Reply` in src-tauri/src/matrix/timeline.rs. */
export type Reply = {
  /** Null until the replied-to message is loaded. */
  name: string | null;
  preview: string | null;
};

/** "12 KB", "3.4 MB", "1.0 GB". */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} bytes`;
  // As `describe_size` in src-tauri/src/matrix/upload.rs: KB whole, MB and
  // GB to one decimal, each only while the rounded number is under 1024.
  const units: [string, number][] = [
    ["KB", 0],
    ["MB", 1],
    ["GB", 1],
  ];
  let value = bytes / 1024;
  for (const [i, [unit, decimals]] of units.entries()) {
    const scale = 10 ** decimals;
    const rounded = Math.round(value * scale) / scale;
    if (rounded < 1024 || i === units.length - 1) return `${rounded.toFixed(decimals)} ${unit}`;
    value /= 1024;
  }
  return `${bytes} bytes`;
}

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

/** Mirrors `WatchedDiff` in src-tauri/src/session.rs: a diff from one watch. */
type WatchedDiff = Diff & { watch: number };

/** Mirrors `Conversation` in src-tauri/src/session.rs. */
type Conversation = { watch: number; lines: Line[] };

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
  const key = roomKey();
  let stopped = false;
  let lines: Line[] | null = null;
  /** The watch our lines come from; diffs from any other are stale. */
  let watch: number | null = null;
  // Diffs that arrive before the first lines come after them, so hold them.
  const early: WatchedDiff[] = [];

  const unlisten = listen<WatchedDiff>(`room:${key}:timeline_diff`, (e) => {
    if (stopped) return;
    if (lines === null) {
      early.push(e.payload);
    } else if (e.payload.watch === watch) {
      lines = apply(lines, e.payload);
      onChange(lines);
    }
  });
  // Listen first, so no change between the two is missed.
  unlisten
    .then(() => invoke<Conversation>("room_timeline", { roomId }))
    .then((first) => {
      if (stopped) return;
      watch = first.watch;
      // Diffs from a watch this one replaced (before a reload) don't fit.
      lines = early.filter((d) => d.watch === watch).reduce(apply, first.lines);
      onChange(lines);
    })
    .catch((e) => !stopped && onError(String(e)));
  return () => {
    stopped = true;
    unlisten.then((f) => f());
  };
}

/**
 * Call `onChange` with the names of who's typing in this room window's room
 * (never you), every time that changes. Returns a function that stops
 * listening.
 */
export function watchTyping(onChange: (names: string[]) => void): () => void {
  const unlisten = listen<string[]>(`room:${roomKey()}:typing`, (e) => onChange(e.payload));
  return () => {
    unlisten.then((f) => f());
  };
}

/** "Bob is typing…", or "" if nobody is. */
export function describeTyping(names: string[]): string {
  if (names.length === 0) return "";
  if (names.length === 1) return `${names[0]} is typing…`;
  if (names.length === 2) return `${names[0]} and ${names[1]} are typing…`;
  return "Several people are typing…";
}

/** The window's label is `room-<key>`; its events are `room:<key>:…`. */
export function roomKey(): string {
  return getCurrentWebviewWindow().label.replace(/^room-/, "");
}
