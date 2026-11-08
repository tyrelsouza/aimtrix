/** Running on macOS, where shortcuts use Cmd rather than Ctrl. */
export const isMac = /Mac/.test(navigator.platform || navigator.userAgent);

/** The key shortcuts use here, for hints: "Cmd" on macOS, else "Ctrl". */
export const shortcutKey = isMac ? "Cmd" : "Ctrl";
