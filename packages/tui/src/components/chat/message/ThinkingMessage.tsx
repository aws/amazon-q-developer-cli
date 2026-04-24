import { StatusBar } from '../status-bar/StatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';

interface ThinkingMessageProps {
  barColor?: string;
}

export const ThinkingMessage: React.FC<ThinkingMessageProps> = ({
  barColor,
}) => {
  const { getColor } = useTheme();
  const secondaryColor = getColor('secondary');
  const dim = getColor('muted');
  const keybindings = useKeybindings();

  return (
    <StatusBar status="thinking" barColor={barColor}>
      <Text>
        {secondaryColor('Thinking...')}
        {dim(` (${keybindings.label('cancelStream')} to cancel)`)}
      </Text>
    </StatusBar>
  );
};
