import { useTheme } from '../../../hooks/useThemeContext.js';
import { Text } from '../text/Text.js';

export enum IconType {
  DOT = 'dot',
  SMALL_DOT = 'small-dot',
  CHEVRON_RIGHT = 'chevron-right',
}

const ICON_MAP: Record<IconType, string> = {
  [IconType.DOT]: '●',
  [IconType.SMALL_DOT]: '·',
  [IconType.CHEVRON_RIGHT]: '❯',
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
