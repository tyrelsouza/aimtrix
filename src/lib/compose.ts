// Turning the IM window's editor into a message, and showing formatted
// messages. The Rust core cleans all HTML; this only builds and decorates it.

/** The text colors on the toolbar, in order. */
export const COLORS: [string, string][] = [
  ["#000000", "Black"],
  ["#ff0000", "Red"],
  ["#0000ff", "Blue"],
  ["#008000", "Green"],
  ["#800080", "Purple"],
  ["#ff8000", "Orange"],
  ["#008080", "Teal"],
  ["#808080", "Gray"],
];

const HEX = /^#[0-9a-f]{6}$/i;

/**
 * The editor's content as a message: `body` is plain text, and `html` is
 * Matrix HTML, or null when nothing is formatted.
 *
 * Only bold, italic, underline, text color and line breaks are kept, so
 * whatever the browser put in the editor, the HTML stays simple.
 */
export function serialize(editor: HTMLElement): { body: string; html: string | null } {
  let body = "";
  let html = "";
  let formatted = false;

  const walk = (node: Node) => {
    for (const child of node.childNodes) {
      if (child.nodeType === Node.TEXT_NODE) {
        const text = child.textContent ?? "";
        body += text;
        html += escape(text);
      } else if (child instanceof HTMLElement) {
        const tag = child.tagName.toLowerCase();
        if (tag === "br") {
          body += "\n";
          html += "<br>";
          continue;
        }
        // The browser wraps new lines in blocks; they start a new line.
        if ((tag === "div" || tag === "p") && body !== "" && !body.endsWith("\n")) {
          body += "\n";
          html += "<br>";
        }
        const [open, close] = wrappers(child);
        if (open) formatted = true;
        html += open;
        walk(child);
        html += close;
      }
    }
  };
  walk(editor);

  // The browser keeps a placeholder break after the last line.
  body = body.replace(/\n+$/, "");
  html = html.replace(/(<br>)+$/, "");
  return { body, html: formatted ? html : null };
}

/** Opening and closing tags for the formatting `el` applies, or "" if none. */
function wrappers(el: HTMLElement): [string, string] {
  const tag = el.tagName.toLowerCase();
  const style = el.style;
  const tags: string[] = [];
  if (tag === "b" || tag === "strong" || style.fontWeight === "bold" || style.fontWeight === "700") {
    tags.push("b");
  }
  if (tag === "i" || tag === "em" || style.fontStyle === "italic") tags.push("i");
  if (tag === "u" || style.textDecoration.includes("underline")) tags.push("u");
  const color = toHex(el.getAttribute("color") ?? style.color);

  let open = tags.map((t) => `<${t}>`).join("");
  let close = tags
    .reverse()
    .map((t) => `</${t}>`)
    .join("");
  if (color) {
    open = `<font data-mx-color="${color}">` + open;
    close += "</font>";
  }
  return [open, close];
}

/** `#rrggbb` from `#rrggbb` or `rgb(r, g, b)`, or null. */
function toHex(color: string): string | null {
  if (HEX.test(color)) return color.toLowerCase();
  const rgb = color.match(/^rgb\((\d+),\s*(\d+),\s*(\d+)\)$/);
  if (!rgb) return null;
  return "#" + rgb.slice(1, 4).map((n) => Number(n).toString(16).padStart(2, "0")).join("");
}

function escape(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * Svelte action for a formatted message: applies Matrix's `data-mx-color`
 * and `data-mx-bg-color`, which browsers ignore, and keeps links from
 * navigating the window. Pass the HTML so it reruns when that changes.
 */
export function mxFormatting(node: HTMLElement, _html: string) {
  const apply = () => {
    for (const el of node.querySelectorAll<HTMLElement>("[data-mx-color]")) {
      if (HEX.test(el.dataset.mxColor ?? "")) el.style.color = el.dataset.mxColor!;
    }
    for (const el of node.querySelectorAll<HTMLElement>("[data-mx-bg-color]")) {
      if (HEX.test(el.dataset.mxBgColor ?? "")) el.style.backgroundColor = el.dataset.mxBgColor!;
    }
    for (const a of node.querySelectorAll<HTMLAnchorElement>("a[href]")) {
      a.title = a.href;
    }
  };
  // Links open nowhere yet; see "Known issues" in PLAN.md.
  const click = (e: MouseEvent) => {
    if ((e.target as Element).closest("a")) e.preventDefault();
  };
  apply();
  node.addEventListener("click", click);
  return {
    update: apply,
    destroy: () => node.removeEventListener("click", click),
  };
}
