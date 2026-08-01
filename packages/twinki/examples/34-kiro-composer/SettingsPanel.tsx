import React from 'react';
import { Box, Text } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';

export function SettingsPanel({
  width,
  height,
  theme,
  yolo,
  engine,
  layoutTitle,
  onActivate,
  onNextTheme,
  onTogglePermissions,
}: {
  width: number;
  height: number;
  theme: ShowcaseTheme;
  yolo: boolean;
  engine: string;
  layoutTitle: string;
  onActivate: () => void;
  onNextTheme: () => void;
  onTogglePermissions: () => void;
}): React.ReactElement {
  return (
    <Box width={width} height={height} flexDirection="column" backgroundColor={theme.panel} onClick={onActivate}>
      <Box height={1} paddingX={1} backgroundColor={theme.raised}>
        <Text color={theme.fg} bold>
          SETTINGS
        </Text>
      </Box>
      <Box paddingX={1} justifyContent="space-between">
        <Text color={theme.muted}>Theme</Text>
        <Text color={theme.accent} bold onClick={onNextTheme}>
          {theme.label} ›
        </Text>
      </Box>
      <Box paddingX={1} justifyContent="space-between">
        <Text color={theme.muted}>Permissions</Text>
        <Text color={yolo ? theme.warning : theme.success} bold onClick={onTogglePermissions}>
          {yolo ? 'YOLO' : 'ASK'} ›
        </Text>
      </Box>
      <Box paddingX={1} justifyContent="space-between">
        <Text color={theme.muted}>Queue prompts</Text>
        <Text color={theme.success} bold>
          ON
        </Text>
      </Box>
      <Box paddingX={1} justifyContent="space-between">
        <Text color={theme.muted}>Engine</Text>
        <Text color={theme.fg}>{engine}</Text>
      </Box>
      <Box paddingX={1} justifyContent="space-between">
        <Text color={theme.muted}>Layout</Text>
        <Text color={theme.fg} wrap="truncate-middle">
          {layoutTitle}
        </Text>
      </Box>
      <Box marginTop={1} paddingX={1}>
        <Text color={yolo ? theme.warning : theme.muted} wrap="wrap">
          {yolo ? 'Tool requests are approved automatically.' : 'Tool requests require confirmation.'}
        </Text>
      </Box>
    </Box>
  );
}
