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
  ARROW_LEFT = 'arrow-left',
  ARROW_UP = 'arrow-up',
  PROGRESS_25 = 'progress-25',
  PROGRESS_25_FILLED = 'progress-25-filled',
  PROGRESS_50_FILLED = 'progress-50-filled',
  PROGRESS_75_FILLED = 'progress-75-filled',
  CHECKMARK = 'checkmark',
  CROSS = 'cross',
  WARNING = 'warning',
  DIAMOND = 'diamond',
  EYE = 'eye',
  SPARKLE = 'sparkle',
  PENCIL = 'pencil',
  WRENCH = 'wrench',
  PAUSE = 'pause',
  LOOP = 'loop',
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
      [IconType.ARROW_LEFT]: glyphs.arrowLeft,
      [IconType.ARROW_UP]: glyphs.arrowUp,
      [IconType.PROGRESS_25]: glyphs.progress0,
      [IconType.PROGRESS_25_FILLED]: glyphs.progress25,
      [IconType.PROGRESS_50_FILLED]: glyphs.progress50,
      [IconType.PROGRESS_75_FILLED]: glyphs.progress75,
      [IconType.CHECKMARK]: glyphs.checkmark,
      [IconType.CROSS]: glyphs.cross,
      [IconType.WARNING]: glyphs.warning,
      [IconType.DIAMOND]: glyphs.diamond,
      [IconType.EYE]: glyphs.eye,
      [IconType.SPARKLE]: glyphs.sparkle,
      [IconType.PENCIL]: glyphs.pencil,
      [IconType.WRENCH]: glyphs.wrench,
      [IconType.PAUSE]: glyphs.pause,
      [IconType.LOOP]: glyphs.loop,
    }),
    [glyphs]
  );

  if (!allowIcons) return <Text> </Text>;

  const icon = ICON_MAP[type];
  return <Text>{colorFn(icon)}</Text>;
};
