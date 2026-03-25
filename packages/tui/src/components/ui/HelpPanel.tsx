import React, { useState, useCallback, useMemo } from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import { visibleWidth } from '../../utils/text-width.js';

interface Command {
  name: string;
  description: string;
  usage: string;
  subcommands?: string[];
}

interface HelpPanelProps {
  commands: Command[];
  onClose: () => void;
}

const GAP = 2;

function commandToRows(
  cmd: Command,
  primary: (s: string) => string,
  dim: (s: string) => string
): Row[] {
  const name = cmd.name.replace(/^\//, '');
  const subs = cmd.subcommands;
  const mainRow: Row = [
    { text: name, color: primary },
    { text: cmd.description, color: dim },
  ];
  if (!subs || subs.length === 0) return [mainRow];
  return [
    mainRow,
    [
      { text: '', color: dim },
      { text: ` ⌙ subcommands: ${subs.join(', ')}`, color: dim },
    ],
  ];
}

export const HelpPanel: React.FC<HelpPanelProps> = ({ commands, onClose }) => {
  const { getColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');

  const maxVisible = Math.max(termHeight - 11, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const q = search.toLowerCase();
  const filtered = search
    ? commands
        .map((c) => ({
          c,
          score: Math.max(
            fuzzyScore(q, c.name.toLowerCase()),
            fuzzyScore(q, c.description.toLowerCase())
          ),
        }))
        .filter(({ score }) => score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ c }) => c)
    : commands;

  const allRows = useMemo(
    () => filtered.flatMap((cmd) => commandToRows(cmd, primary, dim)),
    [filtered, primary, dim]
  );

  const visibleRows = allRows.slice(scrollOffset, scrollOffset + maxVisible);
  const canScrollDown = scrollOffset + maxVisible < allRows.length;

  const maxNameLen = commands.reduce(
    (max, c) => Math.max(max, visibleWidth(c.name.replace(/^\//, ''))),
    0
  );
  const nameCol = Math.max(maxNameLen, 10) + GAP;

  const columns = [{ width: nameCol }, {}];

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  return (
    <Panel
      title="/help"
      onClose={onClose}
      searchable={true}
      onSearchChange={handleSearchChange}
      canScrollUp={scrollOffset > 0}
      canScrollDown={canScrollDown}
      onScrollUp={() => setScrollOffset((p) => Math.max(0, p - 1))}
      onScrollDown={() =>
        setScrollOffset((p) =>
          Math.min(Math.max(0, allRows.length - maxVisible), p + 1)
        )
      }
    >
      <Box marginBottom={1}>
        <Text>{dim('Usage: /')}</Text>
        <Text>{primary('<COMMAND>')}</Text>
        <Text>{dim(' [subcommand]')}</Text>
      </Box>
      {commands.length === 0 ? (
        <Text>{dim('No commands available')}</Text>
      ) : (
        <Table columns={columns} rows={visibleRows} showHeaders={false} />
      )}
    </Panel>
  );
};
