import React from 'react';
import { Box, useInput } from 'ink';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { Divider } from '../divider/Divider.js';
import { Text } from '../text/Text.js';

export interface PanelProps {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
  onTabSwitch?: () => void;
  showTabHint?: boolean;
  footerExtra?: React.ReactNode;
  footerLeft?: React.ReactNode;
  hideTitleDivider?: boolean;
}

export const Panel: React.FC<PanelProps> = ({
  title,
  children,
  onClose,
  onTabSwitch,
  showTabHint = false,
  footerExtra,
  footerLeft,
  hideTitleDivider = false,
}) => {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();

  useInput((_input, key) => {
    if (key.escape) {
      onClose();
    }
    if (key.tab && onTabSwitch) {
      onTabSwitch();
    }
  });

  return (
    <Box flexDirection="column" width={termWidth}>
      <Box paddingX={1}>
        <Text>{getColor('primary')(title)}</Text>
      </Box>
      {!hideTitleDivider && <Divider />}

      <Box flexDirection="column" paddingX={1}>
        {children}
      </Box>

      <Divider />
      <Box justifyContent="space-between" paddingX={1}>
        <Box>
          <Text>
            {getColor('primary')('ESC')} {getColor('secondary')('to close')}
          </Text>
          {footerLeft && <Text>{getColor('secondary')(' | ')}</Text>}
          {footerLeft}
        </Box>
        <Box>
          {footerExtra}
          {footerExtra && showTabHint && (
            <Text>{getColor('secondary')(' | ')}</Text>
          )}
          {showTabHint && (
            <Text>
              {getColor('primary')('Tab')}{' '}
              {getColor('secondary')('to switch view')}
            </Text>
          )}
        </Box>
      </Box>
    </Box>
  );
};
