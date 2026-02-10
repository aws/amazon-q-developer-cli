import { Box } from 'ink';
import { StatusBar } from '../status-bar/StatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

interface ThinkingMessageProps {
  barColor?: string;
}

export const ThinkingMessage: React.FC<ThinkingMessageProps> = ({ barColor }) => {
  const { getColor } = useTheme();
  const seconadryColor = getColor('secondary');

  return (
    <StatusBar status="thinking" barColor={barColor}>
        <Text>{seconadryColor('Thinking...')}</Text>
    </StatusBar>
  );
};
