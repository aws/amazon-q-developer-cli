import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Menu, type MenuItem } from './menu/Menu.js';
import { WelcomeScreen } from '../welcome-screen/WelcomeScreen.js';
import { Divider } from './divider/Divider.js';

interface FirstLaunchUiModeGateProps {
  onPick: (mode: 'tui' | 'lite') => void;
}

const MENU_ITEMS: MenuItem[] = [
  {
    label: 'Full TUI',
    description: 'Chrome, panels, status bar — the standard experience',
  },
  {
    label: 'Lite',
    description: 'Minimal append-only chat that lives in your scrollback',
  },
];

export const FirstLaunchUiModeGate: React.FC<FirstLaunchUiModeGateProps> = ({
  onPick,
}) => {
  const { getColor } = useTheme();
  const primary = getColor('primary');

  const handleSelect = (item: MenuItem) => {
    onPick(item.label === 'Lite' ? 'lite' : 'tui');
  };

  return (
    <Box flexDirection="column" width="100%">
      <WelcomeScreen agent="kiro" mcpServers={[]} animate={false} />

      <Box flexDirection="column" paddingX={1} marginTop={1} width="100%">
        <Divider />
        <Box>
          <Text wrap="wrap">
            {primary('Pick the UI Kiro CLI launches into by default.')}
          </Text>
        </Box>
        <Divider />

        <Box marginTop={1} marginBottom={1}>
          <Text wrap="wrap">
            {primary(
              'You can change this any time from /settings → display. Use /lite or /tui mid-session to switch on the fly.'
            )}
          </Text>
        </Box>

        <Menu
          items={MENU_ITEMS}
          onSelect={handleSelect}
          showSelectedIndicator={true}
          showFooterHints={true}
          visibleItems={2}
        />
      </Box>
    </Box>
  );
};
