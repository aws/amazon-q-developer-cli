import React, { useMemo, useState } from 'react';
import { Box } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useAllowAsciiArt } from '../../hooks/useGlyphs.js';
import {
  getRecentReleases,
  UNICODE_ICONS,
  ASCII_ICONS,
} from '../../constants/feed.js';

interface ChangelogPanelProps {
  onClose: () => void;
}

/** Most-recent releases shown. Matches V1 `/changelog` (`take(2)`). */
const CHANGELOG_RELEASE_LIMIT = 2;

export const ChangelogPanel: React.FC<ChangelogPanelProps> = ({ onClose }) => {
  const { getColor, getUserResponseColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const { allowAsciiArt } = useAllowAsciiArt();

  const icons = allowAsciiArt ? UNICODE_ICONS : ASCII_ICONS;
  const releases = useMemo(
    () => getRecentReleases(CHANGELOG_RELEASE_LIMIT, { icons }),
    [icons]
  );

  // Markdown: `## ✨ What's new in X.Y.Z (date)` per release, joined by `---`.
  const body = useMemo(
    () =>
      releases
        .map((r) =>
          r.content.replace(
            /^\*\*✨ What's new in (.*)\*\*$/m,
            `**✨ What's new in $1 (${r.date})**`
          )
        )
        .join('\n\n---\n\n'),
    [releases]
  );

  // Show enough source lines to fill roughly half the terminal.
  // Markdown rendering adds some expansion but half-height is a safe balance.
  const maxVisible = Math.max(Math.floor(termHeight / 2), 8);

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
