import React from 'react';
import { Box } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import { useTheme } from '../../hooks/useThemeContext.js';

interface TuiPanelProps {
  onClose: () => void;
}

const CONTENT = `## ✨ A new look for the Kiro CLI

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

export const TuiPanel: React.FC<TuiPanelProps> = ({ onClose }) => {
  const { getUserResponseColor } = useTheme();

  return (
    <Panel title="What's new in the TUI" onClose={onClose}>
      <Box flexDirection="column">
        <MarkdownRenderer content={CONTENT} color={getUserResponseColor()} />
      </Box>
    </Panel>
  );
};
