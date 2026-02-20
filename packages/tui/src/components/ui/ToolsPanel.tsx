import React, { useState } from 'react';
import { Box, useInput, Text as InkText } from 'ink';
import { Text } from './text/Text';
import { Divider } from './divider/Divider';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
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

/** Extract a short one-liner from a potentially multi-line description. */
function shortDescription(desc: string, maxLen: number): string {
  const firstLine = desc.split('\n')[0] ?? '';
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

  const maxVisible = Math.max(termHeight - 7, 5);
  const [scrollOffset, setScrollOffset] = useState(0);

  useInput((_input, key) => {
    if (key.escape) {
      onClose();
    } else if (key.upArrow) {
      setScrollOffset((prev) => Math.max(0, prev - 1));
    } else if (key.downArrow) {
      setScrollOffset((prev) =>
        Math.min(Math.max(0, tools.length - maxVisible), prev + 1)
      );
    }
  });

  // Column widths with gap
  const maxNameLen = tools.reduce((max, t) => Math.max(max, t.name.length), 0);
  const nameCol = Math.max(maxNameLen, 12) + GAP;
  const maxSourceLen = tools.reduce(
    (max, t) => Math.max(max, t.source.length),
    0
  );
  const sourceCol = Math.max(maxSourceLen, 10) + GAP;
  const statusCol = 20 + GAP;
  const descCol = Math.max(termWidth - nameCol - sourceCol - statusCol - 2, 10);

  // Sort: built-in first, then alphabetical by source
  const sorted = [...tools].sort((a, b) => {
    if (a.source === 'built-in' && b.source !== 'built-in') return -1;
    if (b.source === 'built-in' && a.source !== 'built-in') return 1;
    return a.source.localeCompare(b.source) || a.name.localeCompare(b.name);
  });

  const visible = sorted.slice(scrollOffset, scrollOffset + maxVisible);
  const canScrollUp = scrollOffset > 0;
  const canScrollDown = scrollOffset + maxVisible < sorted.length;

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

  return (
    <Box flexDirection="column" paddingX={1} paddingY={0} width={termWidth}>
      <Text>
        {primary(
          `/tools · ${tools.length} tool${tools.length === 1 ? '' : 's'}`
        )}
      </Text>
      <Divider />

      {tools.length === 0 ? (
        <Box marginBottom={1}>
          <Text>{dim('No tools available')}</Text>
        </Box>
      ) : (
        <Box flexDirection="column" marginBottom={0}>
          {/* Header row */}
          <Box>
            <Box width={nameCol}>
              <Text>{dim('Name')}</Text>
            </Box>
            <Box width={sourceCol}>
              <Text>{dim('Source')}</Text>
            </Box>
            <Box width={statusCol}>
              <Text>{dim('Status')}</Text>
            </Box>
            <Text>{dim('Description')}</Text>
          </Box>

          {canScrollUp && <Text>{dim('  ↑ more')}</Text>}

          {visible.map((tool) => {
            const st = tool.status ?? 'requires-approval';
            return (
              <Box key={`${tool.source}:${tool.name}`}>
                <Box width={nameCol}>
                  <Text>{primary(tool.name)}</Text>
                </Box>
                <Box width={sourceCol}>
                  <Text>{sourceColor(tool.source)(tool.source)}</Text>
                </Box>
                <Box width={statusCol}>
                  <Text>{statusColor(st)(statusLabels[st])}</Text>
                </Box>
                <Text>{dim(shortDescription(tool.description, descCol))}</Text>
              </Box>
            );
          })}

          {canScrollDown && <Text>{dim('  ↓ more')}</Text>}
        </Box>
      )}

      <Divider />
      <Box justifyContent="space-between">
        <Text>
          {primary('ESC')} {dim('to close')}
          {sorted.length > maxVisible ? dim(' · ↑↓ to scroll') : ''}
        </Text>
      </Box>
    </Box>
  );
};
