import React from 'react';
import { Box, EditorInput, Text } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';

export function ComposerHeader({
  width,
  theme,
  layoutTitle,
  workspace,
  status,
  branch,
  onOpenLayouts,
}: {
  width: number;
  theme: ShowcaseTheme;
  layoutTitle: string;
  workspace: string;
  status: string;
  branch: string;
  onOpenLayouts: () => void;
}): React.ReactElement {
  const statusWidth = width >= 64 ? Math.min(36, Math.max(18, Math.floor(width * 0.36))) : 0;
  const statusText = branch && width >= 96 ? `${status} · ${branch}` : status;
  return (
    <Box width={width} height={1} justifyContent="space-between" backgroundColor={theme.bg}>
      <Box width={Math.max(1, width - statusWidth)} overflow="hidden">
        <Box paddingX={1} backgroundColor={theme.accent}>
          <Text color={theme.accentText} backgroundColor={theme.accent} bold>
            KIRO
          </Text>
        </Box>
        <Text color={theme.fg} backgroundColor={theme.bg} bold>
          {' COMPOSER '}
        </Text>
        <Text color={theme.accent} backgroundColor={theme.bg} bold onClick={onOpenLayouts}>
          {layoutTitle} ▾
        </Text>
        {width >= 88 ? (
          <Text color={theme.muted} backgroundColor={theme.bg} wrap="truncate-middle">
            {' '}
            {workspace}
          </Text>
        ) : null}
      </Box>
      {statusWidth > 0 ? (
        <Box width={statusWidth} justifyContent="flex-end" overflow="hidden">
          <Text
            color={status.startsWith('INVALID') ? theme.danger : theme.muted}
            backgroundColor={theme.bg}
            wrap="truncate-middle"
          >
            {`${statusText} `}
          </Text>
        </Box>
      ) : null}
    </Box>
  );
}

export function ComposerStatusBar({
  width,
  theme,
  copied,
  statusColor,
  status,
  onHelp,
}: {
  width: number;
  theme: ShowcaseTheme;
  copied: boolean;
  statusColor: string;
  status: string;
  onHelp: () => void;
}): React.ReactElement {
  return (
    <Box width={width} height={1} paddingX={1} justifyContent="space-between" backgroundColor={theme.raised}>
      <Text color={copied ? theme.success : statusColor} bold>
        {copied ? 'COPIED' : status}
      </Text>
      <Box>
        <Text color={theme.muted} wrap="truncate">
          {width >= 84
            ? '^L Layouts  ^1 Files  ^2 Git  ^3 Sessions  ^4 Settings  ^R Refresh  '
            : '^L Layouts  ^1-4 Views  ^R Refresh  '}
        </Text>
        <Text color={theme.accent} bold onClick={onHelp}>
          ? HELP
        </Text>
      </Box>
    </Box>
  );
}

export function RenameSessionDialog({
  columns,
  rows,
  value,
  theme,
  onChange,
  onSubmit,
}: {
  columns: number;
  rows: number;
  value: string;
  theme: ShowcaseTheme;
  onChange: (value: string) => void;
  onSubmit: (value: string) => void;
}): React.ReactElement {
  return (
    <Box
      position="absolute"
      left={Math.max(0, Math.floor((columns - Math.min(48, columns - 4)) / 2))}
      top={Math.max(1, Math.floor(rows / 3))}
      width={Math.min(48, columns - 4)}
      height={4}
      paddingX={1}
      flexDirection="column"
      borderStyle="round"
      borderColor={theme.accent}
      backgroundColor={theme.raised}
    >
      <Text color={theme.accent} bold>
        Rename session
      </Text>
      <EditorInput
        value={value}
        onChange={onChange}
        onSubmit={onSubmit}
        isActive
        visibleLines={1}
        width={Math.max(1, Math.min(44, columns - 8))}
        color={theme.fg}
        backgroundColor={theme.panel}
        placeholder="Session name"
        mouseCursor
      />
    </Box>
  );
}
