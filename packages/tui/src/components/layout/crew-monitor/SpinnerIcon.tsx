import React, { useState, useEffect } from 'react';
import { Text } from '../../../renderer.js';
import type { StageState } from './types.js';
import { getStaticIcon, SPINNERS } from './types.js';

export const SpinnerIcon = React.memo(function SpinnerIcon({
  state,
}: {
  state: StageState;
}) {
  const [frame, setFrame] = useState(0);

  useEffect(() => {
    if (state !== 'Executing') return;
    const t = setInterval(() => setFrame((f) => f + 1), 150);
    return () => clearInterval(t);
  }, [state]);

  if (state === 'Executing') {
    return <Text color="magenta">{SPINNERS[frame % SPINNERS.length]}</Text>;
  }
  const { icon, color } = getStaticIcon(state);
  return <Text color={color}>{icon}</Text>;
});
