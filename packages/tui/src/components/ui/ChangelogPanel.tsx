import React, { useMemo, useState } from 'react';
import { Box } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { getRecentReleases } from '../../constants/feed.js';

interface ChangelogPanelProps {
  onClose: () => void;
}

/** Most-recent releases shown. Matches V1 `/changelog` (`take(2)`). */
const CHANGELOG_RELEASE_LIMIT = 2;

export const ChangelogPanel: React.FC<ChangelogPanelProps> = ({ onClose }) => {
  const { getColor, getUserResponseColor } = useTheme();
  const { width: termWidth, height: termHeight } = useTerminalSize();

  const releases = useMemo(
    () => getRecentReleases(CHANGELOG_RELEASE_LIMIT),
    []
  );

  // Markdown: `## ✨ What's new in X.Y.Z (date)` per release, joined by `---`.
  const body = useMemo(
    () =>
      releases
        .map((r) => r.content.replace(/^## (.*)$/m, `## ✨ $1 (${r.date})`))
        .join('\n\n---\n\n'),
    [releases]
  );

  // Scroll window sized to fit the panel inline in the flow. Markdown source
  // lines expand to ~2 terminal rows after header padding / bullet wrapping;
  // wider terminals wrap less, so scale by termWidth/80.
  const widthFactor = Math.max(1, termWidth / 80);
  const maxVisible = Math.max(
    Math.floor(((termHeight - 16) / 2) * widthFactor),
    4
  );

  const lines = useMemo(() => body.split('\n'), [body]);
  const [scrollOffset, setScrollOffset] = useState(0);
  const visibleBody = lines
    .slice(scrollOffset, scrollOffset + maxVisible)
    .join('\n');
  const canScrollUp = scrollOffset > 0;
  const canScrollDown = scrollOffset + maxVisible < lines.length;

  // Empty env / invalid JSON → show fallback.
  if (releases.length === 0) {
    return (
      <Panel title="Changelog" onClose={onClose}>
        <Box flexDirection="column">
          <Text>
            {getColor('muted')('No changelog information available.')}
          </Text>
        </Box>
      </Panel>
    );
  }

  return (
    <Panel
      title="/Changelog"
      onClose={onClose}
      canScrollUp={canScrollUp}
      canScrollDown={canScrollDown}
      onScrollUp={() => setScrollOffset((p) => Math.max(0, p - 1))}
      onScrollDown={() =>
        setScrollOffset((p) =>
          Math.min(Math.max(0, lines.length - maxVisible), p + 1)
        )
      }
    >
      <Box flexDirection="column">
        <MarkdownRenderer
          content={visibleBody}
          color={getUserResponseColor()}
        />
      </Box>
    </Panel>
  );
};
