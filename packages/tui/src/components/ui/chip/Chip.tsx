import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export enum ChipColor {
  PRIMARY = 'primary',
  SECONDARY = 'secondary',
  BRAND = 'brand',
  SUCCESS = 'success',
  WARNING = 'warning',
  ERROR = 'error',
}

/** Chalk color function type */
type ChalkColorFn = (text: string) => string;

interface ChipProps {
  /** The value/name to display */
  value: string;
  /** Color preset (ChipColor enum) or custom chalk function */
  color?: ChipColor | ChalkColorFn;
  /** Prefix text to display before the value */
  prefix?: string;
  /** Whether to wrap the value in parentheses */
  wrap?: boolean;
  /** Whether to show a muted background */
  background?: boolean;
}

export default function Chip({
  value,
  color = ChipColor.PRIMARY,
  prefix,
  wrap = false,
  background = false,
}: ChipProps) {
  const { getColor } = useTheme();

  // Don't render anything if no value
  if (!value) return null;

  // Use custom chalk function if provided, otherwise use theme color
  const valueColor = typeof color === 'function' ? color : getColor(color);

  const content = (
    <Text>
      {prefix && getColor(ChipColor.SECONDARY)(prefix)}
      {wrap && getColor(ChipColor.SECONDARY)('(')}
      {valueColor(value)}
      {wrap && getColor(ChipColor.SECONDARY)(')')}
    </Text>
  );

  if (background) {
    return <Box backgroundColor={getColor('muted').hex}>{content}</Box>;
  }

  return content;
}
