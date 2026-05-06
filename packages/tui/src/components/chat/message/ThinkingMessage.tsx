import { StatusBar } from '../status-bar/StatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeybindings } from '../../../hooks/useKeybindings.js';
import { useAppStore } from '../../../stores/app-store.js';

interface ThinkingMessageProps {
  barColor?: string;
}

/**
 * Inline thinking indicator. When an HTTP retry is in progress (the SDK is backing
 * off between attempts on a transient error), a second line is appended with the
 * attempt counter and countdown text. Renders only "Thinking..." otherwise.
 */
export const ThinkingMessage: React.FC<ThinkingMessageProps> = ({
  barColor,
}) => {
  const { getColor } = useTheme();
  const secondaryColor = getColor('secondary');
  const dim = getColor('muted');
  const warning = getColor('warning');
  const keybindings = useKeybindings();
  const retryStatus = useAppStore((s) => s.retryStatus);

  return (
    <StatusBar status="thinking" barColor={barColor}>
      <Text>
        {secondaryColor('Thinking...')}
        {dim(` (${keybindings.label('cancelStream')} to cancel)`)}
        {retryStatus && (
          <>
            {'\n'}
            {warning(retryStatus.message)}
          </>
        )}
      </Text>
    </StatusBar>
  );
};
