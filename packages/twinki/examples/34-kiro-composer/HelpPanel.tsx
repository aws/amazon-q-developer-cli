import React from 'react';
import { Box, Text } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';

const HELP = [
  ['F1 / Ctrl+/', 'Open or close this help'],
  ['Ctrl+L', 'Choose or create a JSON layout'],
  ['/layout ...', 'Explicitly compose the interface'],
  ['Ctrl+1/2/3/4', 'Files, Git, Sessions, or Settings'],
  ['Settings', 'Cycle theme or switch Ask / YOLO'],
  ['Ctrl+Tab', 'Cycle tabs in the active canvas'],
  ['Ctrl+W', 'Close the active canvas tab'],
  ['Ctrl+R', 'Refresh files and Git'],
  ['Ctrl+G', 'Cycle theme'],
  ['Ctrl+X / Esc', 'Interrupt active turn'],
  ['PgUp / PgDn', 'Scroll the active file or diff'],
  ['Right click', 'Files, diffs, sessions, or canvas tabs'],
  ['Click / drag', 'Focus tabs or resize panes'],
] as const;

export function HelpPanel({
  columns,
  rows,
  theme,
  onClose,
}: {
  columns: number;
  rows: number;
  theme: ShowcaseTheme;
  onClose: () => void;
}): React.ReactElement {
  const width = Math.min(58, Math.max(34, columns - 4));
  const height = HELP.length + 4;
  return (
    <Box
      position="absolute"
      left={Math.max(0, Math.floor((columns - width) / 2))}
      top={Math.max(0, Math.floor((rows - height) / 2))}
      width={width}
      flexDirection="column"
      borderStyle="round"
      borderColor={theme.accent}
      backgroundColor={theme.raised}
      paddingX={1}
    >
      <Text color={theme.accent} bold>
        Kiro Composer
      </Text>
      {HELP.map(([key, detail]) => (
        <Box key={key}>
          <Box width={16}>
            <Text color={theme.warning} bold>
              {key}
            </Text>
          </Box>
          <Text color={theme.fg} wrap="truncate">
            {detail}
          </Text>
        </Box>
      ))}
      <Text color={theme.muted} onClick={onClose}>
        F1, Ctrl+/, Escape, Enter, or click to close
      </Text>
    </Box>
  );
}
