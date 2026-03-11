import React, { useEffect, useState } from 'react';
import { Text } from './../../../renderer.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';

export interface ShimmerTextProps {
  text: string;
  color: string;
}

export const ShimmerText = React.memo(function ShimmerText({
  text,
  color,
}: ShimmerTextProps) {
  const [pos, setPos] = useState(0);
  const paused = useAnimationPaused();

  useEffect(() => {
    if (paused) return;
    const interval = setInterval(() => {
      setPos((p) => (p + 1) % (text.length + 5));
    }, 80);
    return () => clearInterval(interval);
  }, [text.length, paused]);

  return (
    <Text>
      {text.split('').map((char, i) => {
        const dist = Math.abs(i - pos);
        const bright = dist === 0;
        const near = dist <= 2;
        return (
          <Text key={i} color={bright ? 'whiteBright' : near ? 'white' : color}>
            {char}
          </Text>
        );
      })}
    </Text>
  );
});
