import React, { useState, useCallback } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import type { KnowledgeEntry } from '../../stores/app-store.js';

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
      entries.reduce((m, e) => Math.max(m, e.name.length), 0),
      8
    ) + GAP;
  const idCol = 10 + GAP;
  const itemsCol = 8 + GAP;
  const pathCol = Math.max(termWidth - nameCol - idCol - itemsCol - 4, 10);

  const titleSuffix =
    status && entries.length === 0
      ? 'indexing in progress'
      : `${entries.length} entr${entries.length === 1 ? 'y' : 'ies'}`;

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  return (
    <Panel
      title={`/knowledge · ${titleSuffix}`}
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
      {entries.length === 0 && !status ? (
        <Text>{dim('No knowledge base entries')}</Text>
      ) : filtered.length === 0 && search ? (
        <Text>{dim('No matches')}</Text>
      ) : (
        <Box flexDirection="column">
          {filtered.length > 0 && (
            <>
              <Box>
                <Box width={nameCol}>
                  <Text>{dim('Name')}</Text>
                </Box>
                <Box width={idCol}>
                  <Text>{dim('ID')}</Text>
                </Box>
                <Box width={itemsCol}>
                  <Text>{dim('Items')}</Text>
                </Box>
                <Text>{dim('Path')}</Text>
              </Box>
              {visible.map((entry) => {
                const displayPath = entry.path ?? entry.description;
                return (
                  <Box key={entry.id}>
                    <Box width={nameCol}>
                      <Text>{primary(entry.name)}</Text>
                    </Box>
                    <Box width={idCol}>
                      <Text>{brand(entry.id)}</Text>
                    </Box>
                    <Box width={itemsCol}>
                      <Text>{dim(String(entry.item_count))}</Text>
                    </Box>
                    <Text>
                      {dim(
                        displayPath.length > pathCol
                          ? displayPath.slice(0, pathCol - 3) + '...'
                          : displayPath
                      )}
                    </Text>
                  </Box>
                );
              })}
            </>
          )}
          {status && <Text>{dim(status)}</Text>}
        </Box>
      )}
    </Panel>
  );
};
