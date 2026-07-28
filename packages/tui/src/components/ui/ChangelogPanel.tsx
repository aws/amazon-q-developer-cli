import React, { useEffect, useMemo, useState } from 'react';
import { Box } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useAllowAsciiArt, useGlyphs } from '../../hooks/useGlyphs.js';
import {
  getRecentReleases,
  UNICODE_ICONS,
  ASCII_ICONS,
} from '../../constants/feed.js';
import { refreshChangelogFeed } from '../../utils/refresh-feed-cli.js';

interface ChangelogPanelProps {
  onClose: () => void;
}

/** Most-recent releases shown. Matches V1 `/changelog` (`take(2)`). */
const CHANGELOG_RELEASE_LIMIT = 2;

export const ChangelogPanel: React.FC<ChangelogPanelProps> = ({ onClose }) => {
  const { getColor, getUserResponseColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const { allowAsciiArt } = useAllowAsciiArt();
  const glyphs = useGlyphs();

  // Stale-while-revalidate: render the launch-time snapshot immediately,
  // ask the CLI for a fresh fetch, and re-read the feed file if it was
  // updated. The user explicitly asked for the changelog, so freshness
  // is worth the background round-trip.
  const [feedGeneration, setFeedGeneration] = useState(0);
  useEffect(() => {
    // Cancel the spawned refresh if the panel closes before it finishes,
    // so a rapidly opened/closed panel doesn't leave child processes running.
    const controller = new AbortController();
    refreshChangelogFeed(undefined, controller.signal).then((updated) => {
      if (!controller.signal.aborted && updated)
        setFeedGeneration((n) => n + 1);
    });
    return () => {
      controller.abort();
    };
  }, []);

  const icons = allowAsciiArt ? UNICODE_ICONS : ASCII_ICONS;
  const releases = useMemo(
    () => getRecentReleases(CHANGELOG_RELEASE_LIMIT, { icons }),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- feedGeneration invalidates the file re-read
    [icons, feedGeneration]
  );

  // Markdown: `## ✨ What's new in X.Y.Z (date)` per release, joined by `---`.
  const body = useMemo(
    () =>
      releases
        .map((r) =>
          r.content.replace(
            /^\*\*✨ What's new in (.*)\*\*$/m,
            `**${glyphs.sparkle} What's new in $1 (${r.date})**`
          )
        )
        .join('\n\n---\n\n'),
    [releases, glyphs.sparkle]
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
