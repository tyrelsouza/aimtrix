import { describeTyping } from "$lib/timeline";
import { describeUpload, type Upload } from "./upload";

/**
 * The status bar text of a conversation window, most pressing first.
 * It sits below the messages, not above them, so the text doesn't jump.
 */
export function statusText(s: {
  loading: boolean;
  saving: string | null;
  upload: Upload | null;
  notice: string;
  typing: string[];
  name: string;
}): string {
  if (s.loading) return "Loading older messages…";
  if (s.saving !== null) return `Saving ${s.saving}…`;
  if (s.upload?.state === "sending") return describeUpload(s.upload);
  return s.notice || describeTyping(s.typing) || s.name;
}
