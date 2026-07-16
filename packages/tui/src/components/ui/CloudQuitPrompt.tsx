import React, { useCallback } from 'react';
import { Box } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { Text } from './text/Text.js';
import { Menu, type MenuItem } from './menu/Menu.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';

/**
 * `/quit` prompt for a cloud-sandbox session: its agent keeps running after
 * the CLI detaches, so the user chooses keep-running (detach only), turn-off
 * (cancel then detach), or Esc (stay). Outcomes are injected as callbacks so
 * the side-effecting wiring lives at the render site, keeping this
 * presentational. Only reachable in a cloud session.
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
  const glyphs = useGlyphs();
  const dim = getColor('secondary');
  const hint = (k: string, label: string) => (
    <>
      {k} {dim(label)}
    </>
  );

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
    <Panel
      title={getColor('brand')('/quit')}
      onClose={onCancel}
      closeHintLabel="to cancel"
      footerLeft={
        <Text>
          {hint(`${glyphs.arrowUp}${glyphs.arrowDown}`, 'to navigate')}
          {dim(` ${glyphs.smallDot} `)}
          {hint(glyphs.enter, 'to select')}
        </Text>
      }
    >
      <Box height={1} />
      <Box paddingX={1} marginBottom={1}>
        <Text>Would you like the agent to continue working?</Text>
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
