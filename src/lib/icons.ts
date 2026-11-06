// Default Buddy Icons for rooms without an avatar. Original art: plain
// silhouettes on the window face color.

const svg = (body: string) =>
  "data:image/svg+xml," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16">` +
      `<rect width="16" height="16" fill="#d4d0c8"/>${body}</svg>`,
  );

/** One person, for an IM. */
export const DEFAULT_DM_ICON = svg(
  `<circle cx="8" cy="5.5" r="3" fill="#808080"/>` +
    `<path d="M2 16c0-4 2.7-6 6-6s6 2 6 6z" fill="#808080"/>`,
);

/** Two people, for a Chat Room. */
export const DEFAULT_CHAT_ICON = svg(
  `<circle cx="5" cy="6" r="2.3" fill="#a0a0a0"/>` +
    `<path d="M0.5 16c0-3.2 2-5 4.5-5s4.5 1.8 4.5 5z" fill="#a0a0a0"/>` +
    `<circle cx="11" cy="5" r="2.6" fill="#606060"/>` +
    `<path d="M6 16c0-3.6 2.2-5.6 5-5.6s5 2 5 5.6z" fill="#606060"/>`,
);
