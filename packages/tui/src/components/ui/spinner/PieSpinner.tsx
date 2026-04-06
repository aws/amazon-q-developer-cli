import { useState, useEffect } from 'react';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

/** Pie chart animation frames — fills clockwise */
const FRAMES = ['◔', '◑', '◕', '●'];

/** Animation interval in ms */
const INTERVAL = 150;

export interface PieSpinnerProps {
  /** Color — chalk function or will use brand color */
  color?: any;
  /** When true the interval is stopped to save CPU */
  paused?: boolean;
}

export const PieSpinner = ({ color, paused }: PieSpinnerProps) => {
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
