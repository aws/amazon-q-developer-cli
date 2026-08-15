import React, { useState, useCallback, useMemo } from 'react';
import { Box } from '../../renderer.js';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import { useAppStore, type HookInfo } from '../../stores/app-store.js';
import { truncateToWidth } from '../../utils/text-width.js';
import {
  cloudPanelNotice,
  cloudNoticeLineCount,
} from './cloud-panel-notice.js';
import { Feature, features } from '../../features.js';

interface HooksPanelProps {
  hooks: HookInfo[];
  cloudSessionActive?: boolean;
  onClose: () => void;
}

const GAP = 2;

export const HooksPanel: React.FC<HooksPanelProps> = ({
  hooks,
  cloudSessionActive = false,
  onClose,
}) => {
  const { getColor } = useTheme();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const glyphs = useGlyphs();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const brand = getColor('brand');
  const info = getColor('info');

  const cloudNotice = cloudPanelNotice('hooks', cloudSessionActive);
  const noticeLines = cloudNoticeLineCount(
    cloudNotice,
    termWidth - 2,
    hooks.length > 0
  );
  const maxVisible = Math.max(termHeight - 9 - noticeLines, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const sorted = [...hooks].sort(
    (a, b) =>
      a.trigger.localeCompare(b.trigger) ||
      (a.name ?? '').localeCompare(b.name ?? '') ||
      a.command.localeCompare(b.command)
  );

  const q = search.toLowerCase();
  const filtered = search
    ? sorted
        .map((h) => ({
          h,
          score: Math.max(
            fuzzyScore(q, (h.name ?? '').toLowerCase()),
            fuzzyScore(q, h.trigger.toLowerCase()),
            fuzzyScore(q, h.command.toLowerCase()),
            fuzzyScore(q, (h.matcher ?? '').toLowerCase())
          ),
        }))
        .filter(({ score }) => score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ h }) => h)
    : sorted;

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  // Source column, decided ONCE at mount: rendered only inside the
  // cloud_config rollout AND when there is a source fact to show — a cloud
  // session (everything reads "cloud" per UX) or a descriptor-carrying hook
  // (off-cohort and origin-free local sessions are unchanged). Latching
  // means a descriptor push landing while the panel is open cannot insert
  // the column and reflow the rows under the user; reopening re-decides.
  const [showSource] = useState(
    () =>
      features.isEnabled(Feature.CloudConfig) &&
      (cloudSessionActive || hooks.some((h) => h.configSource))
  );

  const nameCol = 20 + GAP;
  const sourceCol = 8 + GAP;
  const triggerCol = 18 + GAP;
  const matcherCol = 16 + GAP;
  const commandCol = Math.max(
    termWidth -
      nameCol -
      (showSource ? sourceCol : 0) -
      triggerCol -
      matcherCol -
      2,
    20
  );

  const columns = [
    { label: 'Name', width: nameCol },
    ...(showSource ? [{ label: 'Source', width: sourceCol }] : []),
    { label: 'Trigger', width: triggerCol },
    { label: 'Command', width: commandCol },
    { label: 'Matcher' },
  ];

  const rows: Row[] = useMemo(
    () =>
      visible.map((hook) => [
        {
          text: truncateToWidth(hook.name ?? '—', nameCol, '...'),
          color: primary,
        },
        ...(showSource
          ? [
              {
                // Cloud session: the whole surface reads "cloud" (per UX);
                // locally the descriptor origin decides.
                text: cloudSessionActive ? 'cloud' : (hook.configSource ?? ''),
                color: dim,
              },
            ]
          : []),
        { text: hook.trigger, color: brand },
        {
          text: truncateToWidth(hook.command, commandCol, '...'),
          color: primary,
        },
        { text: hook.matcher ?? '—', color: hook.matcher ? info : dim },
      ]),
    [
      visible,
      nameCol,
      commandCol,
      showSource,
      cloudSessionActive,
      primary,
      dim,
      brand,
      info,
    ]
  );

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  // Read once per render: set by /config before this panel opens. Drives the
  // footer hint only ('to go back' vs 'to close'); back-navigation itself
  // happens in the close handler.
  const fromConfig = useAppStore((state) => state.configReturnOnEscape);

  return (
    <Panel
      title={`/hooks ${glyphs.smallDot} ${hooks.length} hook${hooks.length === 1 ? '' : 's'}`}
      onClose={onClose}
      closeHintLabel={fromConfig ? 'to go back' : 'to close'}
      searchable={true}
      onSearchChange={handleSearchChange}
      canScrollUp={scrollOffset > 0}
      canScrollDown={canScrollDown}
      onScrollUp={() => setScrollOffset((p) => Math.max(0, p - 1))}
      onScrollDown={() =>
        setScrollOffset((p) =>
          Math.min(Math.max(0, filtered.length - maxVisible), p + 1)
        )
      }
    >
      <Box flexDirection="column">
        {cloudNotice && (
          <Box marginBottom={hooks.length > 0 ? 1 : 0}>
            <Text>{info(cloudNotice)}</Text>
          </Box>
        )}
        {hooks.length === 0 ? (
          <Text>{dim('No hooks configured')}</Text>
        ) : (
          <Table columns={columns} rows={rows} />
        )}
      </Box>
    </Panel>
  );
};
