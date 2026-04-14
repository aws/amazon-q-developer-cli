import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Menu, type MenuItem } from './menu/Menu.js';
import { WelcomeScreen } from '../welcome-screen/WelcomeScreen.js';
import { Divider } from './divider/Divider.js';

interface TrustAllToolsGateProps {
  onAccept: () => void;
  onAcceptAlways: () => void;
  onExit: () => void;
}

const MENU_ITEMS: MenuItem[] = [
  { label: 'No, exit', description: '' },
  { label: 'Yes, I accept', description: '' },
  { label: "Yes, and don't ask again", description: '' },
];

export const TrustAllToolsGate: React.FC<TrustAllToolsGateProps> = ({
  onAccept,
  onAcceptAlways,
  onExit,
}) => {
  const { getColor } = useTheme();
  const warning = getColor('warning');
  const primary = getColor('primary');

  const handleSelect = (item: MenuItem) => {
    if (item.label === 'No, exit') {
      onExit();
    } else if (item.label === "Yes, and don't ask again") {
      onAcceptAlways();
    } else {
      onAccept();
    }
  };

  return (
    <Box flexDirection="column" width="100%">
      <WelcomeScreen agent="kiro" mcpServers={[]} animate={false} />

      <Box flexDirection="column" paddingX={1} marginTop={1} width="100%">
        <Divider />
        <Box>
          <Text wrap="wrap">
            {warning('Warning: Kiro is running in trust all tools mode')}
          </Text>
        </Box>
        <Divider />

        <Box marginTop={1} marginBottom={1}>
          <Text wrap="wrap">
            {primary(
              'In this mode, Kiro will execute all tool calls \u2014 including shell commands, file operations, and MCP tools \u2014 without asking for your approval.'
            )}
          </Text>
        </Box>

        <Box marginBottom={1}>
          <Text wrap="wrap">
            {primary(
              'This mode is intended for sandboxed or disposable environments only. Do not use it on a machine with access to sensitive data or production systems.'
            )}
          </Text>
        </Box>

        <Box marginBottom={1}>
          <Text wrap="wrap">
            {primary(
              'By proceeding, you confirm that you understand the risks and accept responsibility for all actions taken during this session.'
            )}
          </Text>
        </Box>

        <Menu
          items={MENU_ITEMS}
          onSelect={handleSelect}
          onEscape={onExit}
          showSelectedIndicator={true}
          showFooterHints={true}
          visibleItems={3}
        />
      </Box>
    </Box>
  );
};
