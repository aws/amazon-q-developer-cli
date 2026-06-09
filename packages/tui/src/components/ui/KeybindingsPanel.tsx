import React from 'react';
import { Box } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useKeybindings } from '../../hooks/useKeybindings.js';
import { useAppStore } from '../../stores/app-store.js';
import {
  parseKeybinding,
  type KeybindingName,
} from '../../utils/keybindings.js';

interface KeybindingRow {
  /** Displayed name, e.g. "Cancel streaming" */
  name: string;
  /** Short description shown in secondary color */
  description: string;
  /** Which binding to read from useKeybindings() */
  binding: KeybindingName;
  /** Raw default spec (matches the DEFAULTS map in utils/keybindings.ts) */
  defaultSpec: string;
  /** Optional suffix appended after the binding value (e.g. "(×2)" for quit) */
  suffix?: string;
}

const ROWS: readonly KeybindingRow[] = [
  {
    name: 'Cancel streaming',
    description: "Stop the agent's response",
    binding: 'cancelStream',
    defaultSpec: 'esc',
  },
  {
    name: 'Dismiss overlay',
    description: 'Close panels, menus, overlays',
    binding: 'closeMenu',
    defaultSpec: 'esc',
  },
  {
    name: 'Quit',
    description: 'Exit the CLI',
    binding: 'quit',
    defaultSpec: 'ctrl+c',
    suffix: ' (×2)',
  },
] as const;

interface KeybindingsPanelProps {
  onClose: () => void;
}

/**
 * Read-only view of the user-configurable keybindings (cancel, dismiss, quit).
 * Editing happens via the config file — the panel points users there.
 *
 * Values and the [default] label come from `useKeybindings()`, which re-reads
 * settings whenever they change, so remaps made via CLI are reflected live.
 */
export const KeybindingsPanel: React.FC<KeybindingsPanelProps> = ({
  onClose,
}) => {
  const { getColor } = useTheme();
  const { label, cancelStream, closeMenu, quit, toggleInterruptMode } =
    useKeybindings();
  const dim = getColor('secondary');
  const primary = getColor('primary');
  // Drives the ESC hint label only — back-nav itself happens upstream.
  const fromSettings = useAppStore((state) => state.settingsReturnOnEscape);

  const bindings = { cancelStream, closeMenu, quit, toggleInterruptMode };

  return (
    <Panel
      title="/settings – keybindings"
      onClose={onClose}
      closeHintLabel={fromSettings ? 'to go back' : 'to close'}
    >
      <Box flexDirection="column">
        <Box height={1} />
        <Box paddingX={1} marginBottom={1}>
          <Text>
            {dim('Keybindings can be customised in ')}
            {primary('~/.kiro/settings.json')}
          </Text>
        </Box>
        {ROWS.map((row) => {
          const current = bindings[row.binding];
          const defaultParsed = parseKeybinding(row.defaultSpec);
          const isDefault =
            defaultParsed !== null &&
            current.ctrl === defaultParsed.ctrl &&
            current.shift === defaultParsed.shift &&
            current.meta === defaultParsed.meta &&
            current.key === defaultParsed.key;
          const value = `${label(row.binding)}${row.suffix ?? ''}`;
          return (
            <Box key={row.binding} paddingX={1} flexDirection="row">
              <Box width={22}>
                <Text>{row.name}</Text>
              </Box>
              <Box width={40}>
                <Text>{dim(row.description)}</Text>
              </Box>
              <Text>
                {value}
                {isDefault ? dim(' [default]') : ''}
              </Text>
            </Box>
          );
        })}
      </Box>
    </Panel>
  );
};
