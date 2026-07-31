import React from "react";
import { Box, Text } from "twinki";
import type { ContextPoint, PermissionPrompt } from "../types.js";
import type { ShowcaseTheme } from "../themes.js";

const HELP_ROWS = [
  ["Click", "Open files, folders, tabs, themes, and controls"],
  ["Right click", "Open file or editor pane actions"],
  ["Wheel", "Scroll the pane under the pointer"],
  ["Ctrl+Tab", "Switch between Agent and File views"],
  ["Ctrl+S", "Toggle native steering and local queue mode"],
  ["Ctrl+G", "Cycle the active theme"],
  ["Ctrl+X", "Cancel the active agent turn"],
  ["F1 / Ctrl+/", "Open or close this help dialog"],
  ["PgUp/PgDn", "Scroll the file preview"],
  ["Esc", "Interrupt a turn, close a dialog, or return to agent"],
] as const;

export interface ContextAction {
  id: string;
  label: string;
}

export interface ContextMenuProps {
  point: ContextPoint;
  actions: ContextAction[];
  selected: number;
  columns: number;
  rows: number;
  theme: ShowcaseTheme;
  onSelect: (id: string) => void;
}

export function ContextMenu({
  point,
  actions,
  selected,
  columns,
  rows,
  theme,
  onSelect,
}: ContextMenuProps): React.ReactElement {
  const width = Math.min(28, Math.max(18, columns - 2));
  const height = actions.length + 3;
  const left = Math.max(0, Math.min(point.x, columns - width));
  const top = Math.max(0, Math.min(point.y, rows - height));
  return (
    <Box
      position="absolute"
      left={left}
      top={top}
      width={width}
      flexDirection="column"
      borderStyle="round"
      borderColor={theme.accent}
      backgroundColor={theme.raised}
    >
      <Box paddingX={1}>
        <Text color={theme.muted} wrap="truncate-middle">
          {point.label}
        </Text>
      </Box>
      {actions.map((action, index) => (
        <Box
          key={action.id}
          paddingX={1}
          backgroundColor={index === selected ? theme.accent : theme.raised}
          onClick={() => onSelect(action.id)}
        >
          <Text color={index === selected ? theme.accentText : theme.fg} bold={index === selected} wrap="truncate">
            {action.label}
          </Text>
        </Box>
      ))}
    </Box>
  );
}

export interface PermissionPanelProps {
  prompt: PermissionPrompt;
  selected: number;
  columns: number;
  rows: number;
  theme: ShowcaseTheme;
  onSelect: (id: string) => void;
}

export function PermissionPanel({
  prompt,
  selected,
  columns,
  rows,
  theme,
  onSelect,
}: PermissionPanelProps): React.ReactElement {
  const width = Math.min(58, Math.max(24, columns - 4));
  const detail = prompt.detail?.replace(/\s+/g, " ").slice(0, width - 5);
  const height = prompt.choices.length + (detail ? 5 : 4);
  return (
    <Box
      position="absolute"
      left={Math.max(0, Math.floor((columns - width) / 2))}
      top={Math.max(0, Math.floor((rows - height) / 2))}
      width={width}
      flexDirection="column"
      borderStyle="double"
      borderColor={theme.warning}
      backgroundColor={theme.raised}
      paddingX={1}
    >
      <Text color={theme.warning} bold wrap="truncate">
        Permission: {prompt.toolName}
      </Text>
      {detail && (
        <Text color={theme.muted} wrap="truncate">
          {detail}
        </Text>
      )}
      {prompt.choices.map((choice, index) => (
        <Box
          key={choice.id}
          backgroundColor={index === selected ? theme.warning : theme.raised}
          onClick={() => onSelect(choice.id)}
        >
          <Text color={index === selected ? theme.accentText : theme.fg} bold={index === selected} wrap="truncate">
            {`${index + 1}. ${choice.label}`}
          </Text>
        </Box>
      ))}
    </Box>
  );
}

export interface HelpPanelProps {
  columns: number;
  rows: number;
  theme: ShowcaseTheme;
  onClose: () => void;
}

export function HelpPanel({ columns, rows, theme, onClose }: HelpPanelProps): React.ReactElement {
  const width = Math.min(64, Math.max(30, columns - 4));
  const top = Math.max(0, Math.floor((rows - HELP_ROWS.length - 4) / 2));
  return (
    <Box
      position="absolute"
      left={Math.max(0, Math.floor((columns - width) / 2))}
      top={top}
      width={width}
      flexDirection="column"
      borderStyle="round"
      borderColor={theme.accent}
      backgroundColor={theme.raised}
      paddingX={1}
    >
      <Text color={theme.accent} bold>
        Twinki ACP controls
      </Text>
      {HELP_ROWS.map(([key, detail]) => (
        <Box key={key}>
          <Box width={13}>
            <Text color={theme.warning} bold wrap="truncate">
              {key}
            </Text>
          </Box>
          <Text color={theme.fg} wrap="truncate">
            {detail}
          </Text>
        </Box>
      ))}
      <Text color={theme.muted} onClick={onClose}>
        Enter, Escape, or click here to close
      </Text>
    </Box>
  );
}
