/**
 * Extensible feed system for the TUI. Inspired by the Rust-side feed.json,
 * but kept as typed TypeScript to avoid bundling concerns.
 *
 * Add new entry types by extending the FeedEntryType enum and FeedEntry union.
 */

export enum FeedEntryType {
  Announcement = 'announcement',
  // Future: Release = 'release', Tip = 'tip'
}

export interface BaseFeedEntry {
  type: FeedEntryType;
  id: string;
  date: string;
  version: string;
}

export interface AnnouncementEntry extends BaseFeedEntry {
  type: FeedEntryType.Announcement;
  content: string;
  maxShowCount: number;
  priority: number;
  maxLines: number;
}

// Discriminated union — grows as new entry types are added
export type FeedEntry = AnnouncementEntry;

const _ANNOUNCEMENT_CONTENT = `## ✨ A new look for the Kiro CLI

The refreshed terminal experience is now the default — a full TUI with live unified status, omnipresent input, slash commands, \`@\` context, and contextual overlay panels.

### What's new

- **Agent monitor** — \`Ctrl+G\` to visualize subagent activity in real-time
- **Activity tray** — \`Ctrl+X\` to track task progress and queued messages (type while the agent works)
- **Overlay panels** — \`/help\`, \`/context\`, \`/tools\`, \`/mcp\`, \`/knowledge\` as in-place panels
- **Rich tool rendering** — syntax-highlighted diffs, collapsible output (\`Ctrl+O\`)
- **New commands** — \`/theme\` (colors), \`/copy\` (clipboard), \`/spawn\` (parallel agents), \`/feedback\`

### A few things to know

- Some classic mode features are not available:
  - Commands like \`/agent generate\`, \`/prompts create/edit\`
  - Settings, e.g., external diff tool, Vi edit mode
  - Experiments such as \`/tangent\`, \`/checkpoint\`; task lists are now agent-driven
- Custom prompts are now invoked with \`/\` instead of \`@\`; \`@\` is now for file context
- Shell commands — output appears after completion and does not support interactive input

Learn more: https://kiro.dev/docs/cli/tui/

The new TUI is now the default experience.
Prefer classic? \`kiro-cli --classic\` for a single session, or \`kiro-cli settings chat.ui classic\` to make it the default.
Type \`/help\` for all commands · \`/feedback\` to share thoughts`;

export const FEED_ENTRIES: FeedEntry[] = [];

export function getAnnouncements(): AnnouncementEntry[] {
  return FEED_ENTRIES.filter(
    (e): e is AnnouncementEntry => e.type === FeedEntryType.Announcement
  );
}
