import { useMemo } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs, useAllowIcons } from '../../../hooks/useGlyphs.js';
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

export interface IconProps {
  type: IconType;
  color?: any; // chalk function, defaults to primary
}

export const Icon = ({ type, color }: IconProps) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const colorFn = color || getColor('primary');

  const ICON_MAP = useMemo<Record<IconType, string>>(
    () => ({
      [IconType.DOT]: glyphs.dotFilled,
      [IconType.SMALL_DOT]: glyphs.smallDot,
      [IconType.CHEVRON_RIGHT]: glyphs.chevron,
      [IconType.ARROW_DOWN]: glyphs.arrowDown,
      [IconType.ARROW_RIGHT]: glyphs.arrowRight,
      [IconType.PROGRESS_25]: glyphs.progress0,
      [IconType.PROGRESS_25_FILLED]: glyphs.progress25,
      [IconType.PROGRESS_50_FILLED]: glyphs.progress50,
      [IconType.PROGRESS_75_FILLED]: glyphs.progress75,
    }),
    [glyphs]
  );

  if (!allowIcons) return <Text> </Text>;

  const icon = ICON_MAP[type];
  return <Text>{colorFn(icon)}</Text>;
};
