import React, { useCallback } from 'react';
import { Box } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { Text } from './text/Text.js';
import { Menu, type MenuItem } from './menu/Menu.js';
import { useTheme } from '../../hooks/useThemeContext.js';

/**
 * `/quit` prompt for an active cloud-sandbox session.
 *
 * A local `/quit` still exits immediately. A cloud session's agent keeps
 * running in the cloud after the CLI detaches, so on `/quit` we ask which
 * the user wants: keep running (detach only), stop the agent (cancel, then
 * detach), or esc to stay. The outcomes are injected as callbacks so the
 * side-effecting wiring lives at the render site, keeping this component
 * presentational.
 *
 * Remove-when-ready: only reachable when `kiro.isCloudSessionActive()` is
 * true, which is false on released builds (no `cloud-sandbox` cap
 * advertised) — part of the dark-shipped cloud-sandbox feature.
 */
export const CLOUD_QUIT_KEEP_RUNNING = 'Yes (agent continues)';
export const CLOUD_QUIT_TURN_OFF = 'No (agent stops)';

export interface CloudQuitPromptProps {
  /** Detach only — leave the cloud agent running, then exit. */
  onKeepRunning: () => void;
  /** Stop the cloud agent, then detach + exit. */
  onTurnOff: () => void;
  /** Dismiss the prompt and stay attached. */
  onCancel: () => void;
}

export const CloudQuitPrompt: React.FC<CloudQuitPromptProps> = ({
  onKeepRunning,
  onTurnOff,
  onCancel,
}) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');

  const items: MenuItem[] = [
    { label: CLOUD_QUIT_KEEP_RUNNING, description: '' },
    { label: CLOUD_QUIT_TURN_OFF, description: '' },
  ];

  const handleSelect = useCallback(
    (item: MenuItem) => {
      if (item.label === CLOUD_QUIT_TURN_OFF) onTurnOff();
      else onKeepRunning();
    },
    [onKeepRunning, onTurnOff]
  );

  return (
    <Panel title="/quit" onClose={onCancel} closeHintLabel="to cancel">
      <Box height={1} />
      <Box paddingX={1} marginBottom={1}>
        <Text>{dim('Would you like the agent to continue working?')}</Text>
      </Box>
      <Menu
        items={items}
        prefix=""
        onSelect={handleSelect}
        onEscape={onCancel}
        showSelectedIndicator
        showFooterHints={false}
      />
    </Panel>
  );
};
