import React, { useState, useCallback, useMemo } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Divider } from './divider/Divider.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { useInput } from '../../renderer.js';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import type { KnowledgeEntry } from '../../stores/app-store.js';
import { visibleWidth } from '../../utils/text-width.js';

interface KnowledgePanelProps {
  entries: KnowledgeEntry[];
  status: string | null;
  onClose: () => void;
}

const GAP = 2;

export const KnowledgePanel: React.FC<KnowledgePanelProps> = ({
  entries,
  status,
  onClose,
}) => {
  const { getColor } = useTheme();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const brand = getColor('brand');

  const maxVisible = Math.max(termHeight - 9, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');
  const [expanded, setExpanded] = useState(false);

  useInput((_input, key) => {
    if (key.ctrl && _input === 'o') {
      setExpanded((e) => !e);
    }
  });

  const q = search.toLowerCase();
  const filtered = search
    ? entries
        .map((e) => ({
          e,
          score: Math.max(
            fuzzyScore(q, e.name.toLowerCase()),
            fuzzyScore(q, (e.path ?? e.description).toLowerCase())
          ),
        }))
        .filter(({ score }) => score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ e }) => e)
    : entries;

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  const nameCol =
    Math.max(
      entries.reduce((m, e) => Math.max(m, visibleWidth(e.name)), 0),
      8
    ) + GAP;
  const idCol = 10 + GAP;
  const hasIndexing = entries.some((e) => e.indexing);
  const itemsCol = (hasIndexing ? 16 : 8) + GAP;
  const pathCol = Math.max(termWidth - nameCol - idCol - itemsCol - 4, 10);

  const completedCount = entries.filter((e) => !e.indexing).length;
  const indexingCount = entries.filter((e) => e.indexing).length;
  const titleParts: string[] = [];
  if (completedCount > 0) {
    titleParts.push(
      `${completedCount} entr${completedCount === 1 ? 'y' : 'ies'}`
    );
  }
  if (indexingCount > 0) {
    titleParts.push('indexing in progress');
  }
  const titleSuffix =
    titleParts.length > 0 ? titleParts.join(' · ') : 'no entries';

  const columns = [
    { label: 'Name', width: nameCol },
    { label: 'ID', width: idCol },
    { label: 'Status', width: itemsCol },
    { label: 'Path' },
  ];

  const rows: Row[] = useMemo(
    () =>
      visible.map((entry) => {
        const displayPath = entry.path ?? entry.description;
        const truncatedPath =
          visibleWidth(displayPath) > pathCol
            ? '…' + displayPath.slice(-(pathCol - 1))
            : displayPath;
        return [
          { text: entry.name, color: entry.indexing ? dim : primary },
          { text: entry.id, color: entry.indexing ? dim : brand },
          {
            text: entry.items_display ?? `${entry.item_count} items`,
            color: dim,
          },
          { text: truncatedPath, color: dim },
        ];
      }),
    [visible, pathCol, dim, primary, brand]
  );

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  return (
    <Panel
      title={`/knowledge · ${titleSuffix}`}
      onClose={onClose}
      searchable={entries.length > 0}
      onSearchChange={handleSearchChange}
      canScrollUp={scrollOffset > 0}
      canScrollDown={canScrollDown}
      onScrollUp={() => setScrollOffset((p) => Math.max(0, p - 1))}
      onScrollDown={() =>
        setScrollOffset((p) =>
          Math.min(Math.max(0, filtered.length - maxVisible), p + 1)
        )
      }
      footerExtra={
        <Text>
          {primary('ctrl+o')} {dim(expanded ? 'to collapse' : 'to expand')}
        </Text>
      }
    >
      {entries.length === 0 && !status ? (
        <Box flexDirection="column">
          <Text>{dim('No knowledge base entries.')}</Text>
          <Text>
            {dim('Get started: ')}
            {brand('/knowledge add <name> <path>')}
          </Text>
        </Box>
      ) : filtered.length === 0 && search ? (
        <Text>{dim('No matches')}</Text>
      ) : (
        <Table columns={columns} rows={rows} />
      )}
      {expanded && (
        <Box flexDirection="column" marginTop={entries.length > 0 ? 1 : 0}>
          <Divider />
          <Text>{dim('Subcommands:')}</Text>
          <Text>
            {'  '}
            {brand('/knowledge add <name> <path>')}{' '}
            {dim('Add a file or directory')}
          </Text>
          <Text>
            {'  '}
            {brand('/knowledge remove <name|path>')}{' '}
            {dim('Remove specified knowledge base entry')}
          </Text>
          <Text>
            {'  '}
            {brand('/knowledge update <path>')}{' '}
            {dim('Re-index a file or directory')}
          </Text>
          <Text>
            {'  '}
            {brand('/knowledge clear')}{' '}
            {dim('Remove all knowledge base entries')}
          </Text>
          <Text>
            {'  '}
            {brand('/knowledge cancel [operation_id]')}{' '}
            {dim('Cancel a background operation')}
          </Text>
        </Box>
      )}
    </Panel>
  );
};
