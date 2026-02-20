import React, { useState } from 'react';
import { Box, useInput } from 'ink';
import { Text } from './text/Text';
import { Divider } from './divider/Divider';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
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
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const success = getColor('success');
  const warning = getColor('warning');
  const error = getColor('error');

  const maxVisible = Math.max(termHeight - 6, 5);
  const [scrollOffset, setScrollOffset] = useState(0);

  useInput((_input, key) => {
    if (key.escape) {
      onClose();
    } else if (key.upArrow) {
      setScrollOffset((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow) {
      setScrollOffset((prev) =>
        Math.min(Math.max(0, servers.length - maxVisible), prev + 1)
      );
    }
  });

  const maxNameLen = servers.reduce(
    (max, s) => Math.max(max, s.name.length),
    0
  );
  const nameCol = Math.max(maxNameLen, 12) + GAP;
  const statusCol = 14 + GAP;

  const visible = servers.slice(scrollOffset, scrollOffset + maxVisible);
  const canScrollUp = scrollOffset > 0;
  const canScrollDown = scrollOffset + maxVisible < servers.length;

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

  return (
    <Box flexDirection="column" paddingX={1} paddingY={0} width={termWidth}>
      <Text>
        {primary(
          `/mcp · ${servers.length} server${servers.length === 1 ? '' : 's'}`
        )}
      </Text>
      <Divider />

      {servers.length === 0 ? (
        <Box marginBottom={1}>
          <Text>{dim('No MCP servers configured')}</Text>
        </Box>
      ) : (
        <Box flexDirection="column" marginBottom={0}>
          {/* Header row */}
          <Box>
            <Box width={nameCol}>
              <Text>{dim('Name')}</Text>
            </Box>
            <Box width={statusCol}>
              <Text>{dim('Status')}</Text>
            </Box>
            <Text>{dim('Tools')}</Text>
          </Box>

          {canScrollUp && <Text>{dim('  ↑ more')}</Text>}

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

          {canScrollDown && <Text>{dim('  ↓ more')}</Text>}
        </Box>
      )}

      <Divider />
      <Box justifyContent="space-between">
        <Text>
          {primary('ESC')} {dim('to close')}
          {servers.length > maxVisible ? dim(' · ↑↓ to scroll') : ''}
        </Text>
      </Box>
    </Box>
  );
};
