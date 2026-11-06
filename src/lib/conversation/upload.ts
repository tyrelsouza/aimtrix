import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { roomKey } from "$lib/timeline";

/** Mirrors `Upload` in src-tauri/src/session.rs. */
export type Upload =
  | { state: "sending"; name: string; index: number; count: number; sent: number; total: number }
  | { state: "done" }
  | { state: "failed"; message: string }
  | { state: "cancelled" };

/** Call `onChange` as files sent from this window go. Returns a function that stops listening. */
export function watchUpload(onChange: (upload: Upload) => void): () => void {
  const unlisten = listen<Upload>(`room:${roomKey()}:upload`, (e) => onChange(e.payload));
  return () => {
    unlisten.then((f) => f());
  };
}

/** "Sending photo.jpg… 45%", with "(2 of 3)" when there are several. */
export function describeUpload(upload: Upload & { state: "sending" }): string {
  const of = upload.count > 1 ? ` (${upload.index} of ${upload.count})` : "";
  const percent = upload.total > 0 ? ` ${Math.floor((upload.sent * 100) / upload.total)}%` : "";
  return `Sending ${upload.name}${of}…${percent}`;
}

/**
 * Send files from this room's window: the ones in `paths`, or ones picked
 * in an Open dialog. Progress arrives through `watchUpload`.
 */
export async function sendFiles(roomId: string, paths: string[] | null = null): Promise<void> {
  await invoke("send_files", { roomId, paths });
}

/**
 * Send a pasted picture, after Rust asks whether to. The bytes go as the
 * raw request body, which is much faster than JSON for big pictures.
 */
export async function sendPasted(roomId: string, file: File): Promise<void> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  await invoke("send_pasted", bytes, {
    headers: { "x-room-id": roomId, "x-mime-type": file.type },
  });
}

/**
 * Send the picture on the system clipboard, after Rust asks whether to.
 * For pastes the webview doesn't hand over, which is all of them on
 * WebKitGTK. Does nothing if there's no picture.
 */
export async function pasteClipboardPicture(roomId: string): Promise<void> {
  await invoke("paste_picture", { roomId });
}

/** The pictures in a paste, if any. */
export function pastedPictures(e: ClipboardEvent): File[] {
  return Array.from(e.clipboardData?.files ?? []).filter((f) => f.type.startsWith("image/"));
}

/**
 * Send files dropped on the window. `onHover` says whether files are being
 * dragged over it; `onError` gets anything that stops them being sent.
 */
export function watchFileDrops(
  roomId: string,
  onHover: (hovering: boolean) => void,
  onError: (message: string) => void,
): () => void {
  const unlisten = getCurrentWebview().onDragDropEvent((e) => {
    switch (e.payload.type) {
      case "enter":
      case "over":
        onHover(true);
        break;
      case "leave":
        onHover(false);
        break;
      case "drop":
        onHover(false);
        if (e.payload.paths.length > 0) {
          sendFiles(roomId, e.payload.paths).catch((err) => onError(String(err)));
        }
        break;
    }
  });
  return () => {
    unlisten.then((f) => f());
  };
}
