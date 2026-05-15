import React, { useState, useEffect } from 'react';
import { Text } from '../../../renderer.js';
import type { StageState } from './types.js';
import {
  useGlyphs,
  useSpinners,
  useAllowIcons,
} from '../../../hooks/useGlyphs.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';

export const SpinnerIcon = React.memo(function SpinnerIcon({
  state,
}: {
  state: StageState;
}) {
  const glyphs = useGlyphs();
  const spinners = useSpinners();
  const { allowIcons } = useAllowIcons();
  const frames = spinners.quarterSpinner;
  const [frame, setFrame] = useState(0);
  const paused = useAnimationPaused();

  useEffect(() => {
    if (state !== 'Executing') return;
    if (paused) return;
    const t = setInterval(() => setFrame((f) => f + 1), 150);
    return () => clearInterval(t);
  }, [state, paused]);

  if (state === 'Executing') {
    return <Text color="magenta">{frames[frame % frames.length]}</Text>;
  }

  switch (state) {
    case 'Completed':
      return <Text color="gray">{!allowIcons ? '' : glyphs.checkmark}</Text>;
    case 'Failed':
      return <Text color="red">{!allowIcons ? '' : glyphs.cross}</Text>;
    default:
      return <Text color="gray">{!allowIcons ? '' : glyphs.dotEmpty}</Text>;
  }
});
