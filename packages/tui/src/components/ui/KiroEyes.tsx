import React, { useState, useEffect, useMemo } from 'react';
import { Text } from './../../renderer.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useAnimationPaused } from '../../contexts/AnimationPausedContext.js';

interface KiroEyesProps {
  isWaiting?: boolean;
  message?: string;
}

export const KiroEyes: React.FC<KiroEyesProps> = ({
  isWaiting = false,
  message,
}) => {
  const [frameIndex, setFrameIndex] = useState(0);
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const primaryColor = getColor('primary');
  const animationPaused = useAnimationPaused();

  const IDLE_FRAMES: [string, number][] = useMemo(
    () => [
      [`${glyphs.eye} ${glyphs.eye}`, 3000],
      ['– –', 150],
    ],
    [glyphs]
  );

  const THINKING_FRAMES: [string, number, string][] = useMemo(
    () => [
      [`${glyphs.eye} ${glyphs.eye}`, 2500, 'thinking...'],
      ['– –', 150, 'thinking...'],
      [`${glyphs.eye} ${glyphs.eye}`, 2500, 'thinking...'],
      [`${glyphs.eye} –`, 800, 'hmm...'],
      ['– –', 150, 'processing...'],
      [`${glyphs.eye} ${glyphs.eye}`, 2500, 'thinking...'],
      [`– ${glyphs.eye}`, 800, 'hmm...'],
      ['– –', 150, 'processing...'],
    ],
    [glyphs]
  );

  // Reset frame when switching modes
  useEffect(() => {
    setFrameIndex(0);
  }, [isWaiting]);

  useEffect(() => {
    if (animationPaused) return;
    const frames = isWaiting ? THINKING_FRAMES : IDLE_FRAMES;
    const frame = frames[frameIndex];
    if (!frame) return;

    const duration = frame[1];
    const timer = setTimeout(() => {
      setFrameIndex((i) => (i + 1) % frames.length);
    }, duration);

    return () => clearTimeout(timer);
  }, [frameIndex, isWaiting, animationPaused, IDLE_FRAMES, THINKING_FRAMES]);

  if (message) {
    return <Text>{primaryColor(message)}</Text>;
  }

  if (isWaiting) {
    const frame = THINKING_FRAMES[frameIndex % THINKING_FRAMES.length];
    if (!frame) return null;
    const [eyes, , label] = frame;
    return (
      <Text>
        <Text color="white">{eyes}</Text> <Text>{primaryColor(label)}</Text>
      </Text>
    );
  }

  // Idle - just eyes blinking
  const frame = IDLE_FRAMES[frameIndex % IDLE_FRAMES.length];
  if (!frame) return null;
  const [eyes] = frame;
  return <Text color="white">{eyes}</Text>;
};
