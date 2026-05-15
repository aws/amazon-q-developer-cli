import { useState, useEffect } from 'react';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useSpinners } from '../../../hooks/useGlyphs.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';

/** Animation interval in ms */
const INTERVAL = 100;

export interface SpinnerProps {
  /** Color - chalk function or will use brand color */
  color?: any;
  /** When true the interval is stopped to save CPU */
  paused?: boolean;
}

export const Spinner = ({ color, paused }: SpinnerProps) => {
  const { getColor } = useTheme();
  const spinners = useSpinners();
  const frames = spinners.brailleFill;
  const [frameIndex, setFrameIndex] = useState(0);
  const globalPaused = useAnimationPaused();

  const colorFn = color || getColor('brand');

  useEffect(() => {
    if (paused || globalPaused) return;
    const timer = setInterval(() => {
      setFrameIndex((prev) => (prev + 1) % frames.length);
    }, INTERVAL);

    return () => clearInterval(timer);
  }, [paused, globalPaused, frames.length]);

  const displayIndex = paused || globalPaused ? frames.length - 1 : frameIndex;

  return <Text>{colorFn(frames[displayIndex])}</Text>;
};
