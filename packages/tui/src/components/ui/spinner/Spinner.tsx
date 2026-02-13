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
}

export const Spinner = ({ color }: SpinnerProps) => {
  const { getColor } = useTheme();
  const [frameIndex, setFrameIndex] = useState(0);

  const colorFn = color || getColor('brand');

  useEffect(() => {
    const timer = setInterval(() => {
      setFrameIndex((prev) => (prev + 1) % FRAMES.length);
    }, INTERVAL);

    return () => clearInterval(timer);
  }, []);

  return <Text>{colorFn(FRAMES[frameIndex])}</Text>;
};
