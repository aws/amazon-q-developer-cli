import React, { useState, useCallback } from 'react';
import { Box } from 'ink';
import { Text } from './text/Text.js';
import { Panel } from './panel/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';

interface PromptArg {
  name: string;
  description?: string;
  required?: boolean;
}

interface Prompt {
  name: string;
  description?: string;
  arguments: PromptArg[];
  serverName: string;
}

interface PromptsPanelProps {
  prompts: Prompt[];
  onClose: () => void;
}

const GAP = 2;

function shortDescription(desc: string | undefined, maxLen: number): string {
  if (!desc) return '(file prompt)';
  const firstLine = desc.trim().split('\n')[0] ?? '';
  const clean = firstLine.replace(/\s+/g, ' ').trim();
  if (clean.length <= maxLen) return clean;
  return clean.slice(0, maxLen - 3) + '...';
}

export const PromptsPanel = React.memo(function PromptsPanel({ prompts, onClose }: PromptsPanelProps) {
  const { getColor } = useTheme();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const brand = getColor('brand');
  const info = getColor('info');

  const maxVisible = Math.max(termHeight - 9, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const sorted = [...prompts].sort((a, b) =>
    a.serverName.localeCompare(b.serverName) || a.name.localeCompare(b.name)
  );

  const q = search.toLowerCase();
  const filtered = search
    ? sorted.filter((p) => p.name.toLowerCase().includes(q) || p.serverName.toLowerCase().includes(q) || (p.description ?? '').toLowerCase().includes(q))
    : sorted;

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  const maxNameLen = prompts.reduce((max, p) => Math.max(max, p.name.length + 1), 0);
  const nameCol = Math.max(maxNameLen, 12) + GAP;
  const maxServerLen = prompts.reduce((max, p) => Math.max(max, p.serverName.length), 0);
  const serverCol = Math.max(maxServerLen, 10) + GAP;
  const argsCol = 24 + GAP;
  const descCol = Math.max(termWidth - nameCol - serverCol - argsCol - 2, 10);

  const serverColor = (serverName: string) => serverName === 'built-in' ? brand : info;

  const handleSearchChange = useCallback((s: string) => { setSearch(s); setScrollOffset(0); }, []);

  return (
    <Panel
      title={`/prompts · ${prompts.length} prompt${prompts.length === 1 ? '' : 's'}`}
      onClose={onClose}
      searchable={true}
      onSearchChange={handleSearchChange}
      canScrollUp={scrollOffset > 0}
      canScrollDown={canScrollDown}
      onScrollUp={() => setScrollOffset((p) => Math.max(0, p - 1))}
      onScrollDown={() => setScrollOffset((p) => Math.min(Math.max(0, filtered.length - maxVisible), p + 1))}
    >
      {prompts.length === 0 ? (
        <Text>{dim('No prompts available')}</Text>
      ) : (
        <Box flexDirection="column">
          <Box>
            <Box width={nameCol}><Text>{dim('Name')}</Text></Box>
            <Box width={serverCol}><Text>{dim('Server')}</Text></Box>
            <Box width={argsCol}><Text>{dim('Arguments (* = required)')}</Text></Box>
            <Text>{dim('Description')}</Text>
          </Box>
          {visible.map((prompt) => {
            const args = prompt.arguments.length > 0
              ? prompt.arguments.map((a) => a.required ? a.name + '*' : a.name).join(', ')
              : '';
            return (
              <Box key={`${prompt.serverName}:${prompt.name}`}>
                <Box width={nameCol}><Text>{primary('/' + prompt.name)}</Text></Box>
                <Box width={serverCol}><Text>{serverColor(prompt.serverName)(prompt.serverName)}</Text></Box>
                <Box width={argsCol}><Text>{dim(args.length > argsCol - 2 ? args.slice(0, argsCol - 5) + '...' : args)}</Text></Box>
                <Text>{dim(shortDescription(prompt.description, descCol))}</Text>
              </Box>
            );
          })}
        </Box>
      )}
    </Panel>
  );
});
