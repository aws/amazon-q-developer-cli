import React, { useState, useCallback, useMemo } from 'react';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import type { HookInfo } from '../../stores/app-store.js';
import { truncateToWidth } from '../../utils/text-width.js';

interface HooksPanelProps {
  hooks: HookInfo[];
  onClose: () => void;
}

const GAP = 2;

export const HooksPanel: React.FC<HooksPanelProps> = ({ hooks, onClose }) => {
  const { getColor } = useTheme();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const brand = getColor('brand');
  const info = getColor('info');

  const maxVisible = Math.max(termHeight - 9, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const sorted = [...hooks].sort(
    (a, b) =>
      a.trigger.localeCompare(b.trigger) || a.command.localeCompare(b.command)
  );

  const q = search.toLowerCase();
  const filtered = search
    ? sorted
        .map((h) => ({
          h,
          score: Math.max(
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

  const triggerCol = 20 + GAP;
  const matcherCol = 20 + GAP;
  const commandCol = Math.max(termWidth - triggerCol - matcherCol - 2, 20);

  const columns = [
    { label: 'Trigger', width: triggerCol },
    { label: 'Command', width: commandCol },
    { label: 'Matcher' },
  ];

  const rows: Row[] = useMemo(
    () =>
      visible.map((hook) => [
        { text: hook.trigger, color: brand },
        {
          text: truncateToWidth(hook.command, commandCol, '...'),
          color: primary,
        },
        { text: hook.matcher ?? '—', color: hook.matcher ? info : dim },
      ]),
    [visible, commandCol, primary, dim, brand, info]
  );

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  return (
    <Panel
      title={`/hooks · ${hooks.length} hook${hooks.length === 1 ? '' : 's'}`}
      onClose={onClose}
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
      {hooks.length === 0 ? (
        <Text>{dim('No hooks configured')}</Text>
      ) : (
        <Table columns={columns} rows={rows} />
      )}
    </Panel>
  );
};
