import { useTheme } from '../../../hooks/useThemeContext.js';
import { Text } from '../text/Text.js';

export enum IconType {
  DOT = 'dot',
  SMALL_DOT = 'small-dot',
  CHEVRON_RIGHT = 'chevron-right',
  ARROW_DOWN = 'arrow-down',
  ARROW_RIGHT = 'arrow-right',
  PROGRESS_25 = 'progress-25',
  PROGRESS_25_FILLED = 'progress-25-filled',
  PROGRESS_50_FILLED = 'progress-50-filled',
  PROGRESS_75_FILLED = 'progress-75-filled',
}

const ICON_MAP: Record<IconType, string> = {
  [IconType.DOT]: '●',
  [IconType.SMALL_DOT]: '·',
  [IconType.CHEVRON_RIGHT]: '❯',
  [IconType.ARROW_DOWN]: '↓',
  [IconType.ARROW_RIGHT]: '▸',
  [IconType.PROGRESS_25]: '◷',
  [IconType.PROGRESS_25_FILLED]: '◔',
  [IconType.PROGRESS_50_FILLED]: '◑',
  [IconType.PROGRESS_75_FILLED]: '◕',
};

export interface IconProps {
  type: IconType;
  color?: any; // chalk function, defaults to primary
}

export const Icon = ({ type, color }: IconProps) => {
  const { getColor } = useTheme();
  const colorFn = color || getColor('primary');
  const icon = ICON_MAP[type];
  return <Text>{colorFn(icon)}</Text>;
};
