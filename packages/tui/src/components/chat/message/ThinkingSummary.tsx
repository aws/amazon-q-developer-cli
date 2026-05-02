import { StatusBar } from '../status-bar/StatusBar';
import { Text } from '../../ui/text/Text';
import { useTheme } from '../../../hooks/useThemeContext';

interface ThinkingSummaryProps {
  text: string;
  barColor?: string;
}

export const ThinkingSummary: React.FC<ThinkingSummaryProps> = ({
  text,
  barColor,
}) => {
  const { getColor } = useTheme();
  const dimColor = getColor('secondary');
  const truncated = text.length > 120 ? text.slice(0, 120) + '...' : text;
  return (
    <StatusBar status="thinking" barColor={barColor}>
      <Text>{dimColor(`💭 ${truncated}`)}</Text>
    </StatusBar>
  );
};
