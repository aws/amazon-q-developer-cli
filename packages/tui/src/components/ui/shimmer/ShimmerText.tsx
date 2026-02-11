import React, { useEffect, useState } from 'react';
import { Text } from 'ink';

export interface ShimmerTextProps {
  text: string;
  color: string;
}

export const ShimmerText = React.memo(function ShimmerText({ text, color }: ShimmerTextProps) {
  const [pos, setPos] = useState(0);

  useEffect(() => {
    const interval = setInterval(() => {
      setPos((p) => (p + 1) % (text.length + 5));
    }, 80);
    return () => clearInterval(interval);
  }, [text.length]);

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
