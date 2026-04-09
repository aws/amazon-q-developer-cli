import { StatusBar } from '../status-bar/StatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

interface ThinkingMessageProps {
  barColor?: string;
}

export const ThinkingMessage: React.FC<ThinkingMessageProps> = ({
  barColor,
}) => {
  const { getColor } = useTheme();
  const secondaryColor = getColor('secondary');
  const dim = getColor('muted');

  return (
    <StatusBar status="thinking" barColor={barColor}>
      <Text>
        {secondaryColor('Thinking...')}
        {dim(' (esc to cancel)')}
      </Text>
    </StatusBar>
  );
};
