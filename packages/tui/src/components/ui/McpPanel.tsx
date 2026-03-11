import React, { useState, useCallback } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import type { McpServerInfo } from '../../stores/app-store.js';

interface McpPanelProps {
  servers: McpServerInfo[];
  onClose: () => void;
}

const statusLabels: Record<McpServerInfo['status'], string> = {
  running: '● running',
  loading: '◌ loading',
  failed: '✕ failed',
  disabled: '○ disabled',
};

const GAP = 2;

export const McpPanel: React.FC<McpPanelProps> = ({ servers, onClose }) => {
  const { getColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const success = getColor('success');
  const warning = getColor('warning');
  const error = getColor('error');

  const maxVisible = Math.max(termHeight - 9, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const q = search.toLowerCase();
  const filtered = search
    ? servers
        .map((s) => ({
          s,
          score: Math.max(
            fuzzyScore(q, s.name.toLowerCase()),
            fuzzyScore(q, s.status.toLowerCase())
          ),
        }))
        .filter(({ score }) => score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ s }) => s)
    : servers;

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  const maxNameLen = servers.reduce(
    (max, s) => Math.max(max, s.name.length),
    0
  );
  const nameCol = Math.max(maxNameLen, 12) + GAP;
  const statusCol = 14 + GAP;

  const statusColor = (status: McpServerInfo['status']) => {
    switch (status) {
      case 'running':
        return success;
      case 'loading':
        return warning;
      case 'failed':
        return error;
      case 'disabled':
        return dim;
    }
  };

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  return (
    <Panel
      title={`/mcp · ${servers.length} server${servers.length === 1 ? '' : 's'}`}
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
      {servers.length === 0 ? (
        <Text>{dim('No MCP servers configured')}</Text>
      ) : (
        <Box flexDirection="column">
          <Box>
            <Box width={nameCol}>
              <Text>{dim('Name')}</Text>
            </Box>
            <Box width={statusCol}>
              <Text>{dim('Status')}</Text>
            </Box>
            <Text>{dim('Tools')}</Text>
          </Box>
          {visible.map((server) => (
            <Box key={server.name}>
              <Box width={nameCol}>
                <Text>{primary(server.name)}</Text>
              </Box>
              <Box width={statusCol}>
                <Text>
                  {statusColor(server.status)(statusLabels[server.status])}
                </Text>
              </Box>
              <Text>
                {dim(
                  `${server.toolCount} tool${server.toolCount === 1 ? '' : 's'}`
                )}
              </Text>
            </Box>
          ))}
        </Box>
      )}
    </Panel>
  );
};
