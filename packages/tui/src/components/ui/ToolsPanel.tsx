import React, { useState, useCallback, useMemo } from 'react';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import type { ToolInfo } from '../../stores/app-store.js';

interface ToolsPanelProps {
  tools: ToolInfo[];
  onClose: () => void;
}

const statusLabels: Record<ToolInfo['status'], string> = {
  allowed: '● allowed',
  'requires-approval': '◌ approval required',
  denied: '✕ denied',
};

function shortDescription(desc: string, maxLen: number): string {
  const firstLine = desc.trim().split('\n')[0] ?? '';
  const firstSentence = firstLine.split('. ')[0] ?? firstLine;
  const clean = firstSentence.replace(/\s+/g, ' ').trim();
  if (clean.length <= maxLen) return clean;
  return clean.slice(0, maxLen - 3) + '...';
}

const GAP = 2;

export const ToolsPanel: React.FC<ToolsPanelProps> = ({ tools, onClose }) => {
  const { getColor } = useTheme();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const brand = getColor('brand');
  const info = getColor('info');
  const success = getColor('success');
  const warning = getColor('warning');
  const error = getColor('error');

  const maxVisible = Math.max(termHeight - 9, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const sorted = [...tools].sort((a, b) => {
    if (a.source === 'built-in' && b.source !== 'built-in') return -1;
    if (b.source === 'built-in' && a.source !== 'built-in') return 1;
    return a.source.localeCompare(b.source) || a.name.localeCompare(b.name);
  });

  const q = search.toLowerCase();
  const filtered = search
    ? sorted
        .map((t) => ({
          t,
          score: Math.max(
            fuzzyScore(q, t.name.toLowerCase()),
            fuzzyScore(q, t.source.toLowerCase()),
            fuzzyScore(q, t.description.toLowerCase())
          ),
        }))
        .filter(({ score }) => score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ t }) => t)
    : sorted;

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  const maxNameLen = tools.reduce((max, t) => Math.max(max, t.name.length), 0);
  const nameCol = Math.max(maxNameLen, 12) + GAP;
  const maxSourceLen = tools.reduce(
    (max, t) => Math.max(max, t.source.length),
    0
  );
  const sourceCol = Math.max(maxSourceLen, 10) + GAP;
  const statusCol = 20 + GAP;
  const descCol = Math.max(termWidth - nameCol - sourceCol - statusCol - 2, 10);

  const statusColor = (status: ToolInfo['status']) => {
    switch (status) {
      case 'allowed':
        return success;
      case 'requires-approval':
        return warning;
      case 'denied':
        return error;
    }
  };
  const sourceColor = (source: string) =>
    source === 'built-in' ? brand : info;

  const columns = [
    { label: 'Name', width: nameCol },
    { label: 'Source', width: sourceCol },
    { label: 'Status', width: statusCol },
    { label: 'Description' },
  ];

  const rows: Row[] = useMemo(
    () =>
      visible.map((tool) => {
        const st = tool.status ?? 'requires-approval';
        return [
          { text: tool.name, color: primary },
          { text: tool.source, color: sourceColor(tool.source) },
          { text: statusLabels[st], color: statusColor(st) },
          { text: shortDescription(tool.description, descCol), color: dim },
        ];
      }),
    [visible, descCol, primary, dim, brand, info, success, warning, error]
  );

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  return (
    <Panel
      title={`/tools · ${tools.length} tool${tools.length === 1 ? '' : 's'}`}
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
      {tools.length === 0 ? (
        <Text>{dim('No tools available')}</Text>
      ) : (
        <Table columns={columns} rows={rows} />
      )}
    </Panel>
  );
};
