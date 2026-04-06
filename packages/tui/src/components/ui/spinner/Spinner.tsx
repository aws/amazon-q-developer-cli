import { useState, useEffect } from 'react';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

/** Braille dot animation frames - fills clockwise from upper-left */
const FRAMES = ['⠀', '⠁', '⠉', '⠙', '⠹', '⢹', '⣹', '⣽', '⣿'];

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
  const [frameIndex, setFrameIndex] = useState(0);

  const colorFn = color || getColor('brand');

  useEffect(() => {
    if (paused) return;
    const timer = setInterval(() => {
      setFrameIndex((prev) => (prev + 1) % FRAMES.length);
    }, INTERVAL);

    return () => clearInterval(timer);
  }, [paused]);

  return <Text>{colorFn(FRAMES[frameIndex])}</Text>;
};
