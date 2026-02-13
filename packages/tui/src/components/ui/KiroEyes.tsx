import React, { useState, useEffect } from 'react';
import { Text } from 'ink';
import { useTheme } from '../../hooks/useTheme.js';

// Idle: just normal blinking
const IDLE_FRAMES: [string, number][] = [
  ['◉ ◉', 3000],
  ['– –', 150],
];

// Thinking: winks and expressions
const THINKING_FRAMES: [string, number, string][] = [
  ['◉ ◉', 2500, 'thinking...'],
  ['– –', 150, 'thinking...'],
  ['◉ ◉', 2500, 'thinking...'],
  ['◉ –', 800, 'hmm...'],
  ['– –', 150, 'processing...'],
  ['◉ ◉', 2500, 'thinking...'],
  ['– ◉', 800, 'hmm...'],
  ['– –', 150, 'processing...'],
];

interface KiroEyesProps {
  isWaiting?: boolean;
  message?: string;
}

export const KiroEyes: React.FC<KiroEyesProps> = ({
  isWaiting = false,
  message,
}) => {
  const [frameIndex, setFrameIndex] = useState(0);
  const { colors } = useTheme();

  // Reset frame when switching modes
  useEffect(() => {
    setFrameIndex(0);
  }, [isWaiting]);

  useEffect(() => {
    const frames = isWaiting ? THINKING_FRAMES : IDLE_FRAMES;
    const frame = frames[frameIndex];
    if (!frame) return;

    const duration = frame[1];
    const timer = setTimeout(() => {
      setFrameIndex((i) => (i + 1) % frames.length);
    }, duration);

    return () => clearTimeout(timer);
  }, [frameIndex, isWaiting]);

  if (message) {
    return <Text color={colors.primary}>{message}</Text>;
  }

  if (isWaiting) {
    const frame = THINKING_FRAMES[frameIndex % THINKING_FRAMES.length];
    if (!frame) return null;
    const [eyes, , label] = frame;
    return (
      <Text>
        <Text color="white">{eyes}</Text>{' '}
        <Text color={colors.primary}>{label}</Text>
      </Text>
    );
  }

  // Idle - just eyes blinking
  const frame = IDLE_FRAMES[frameIndex % IDLE_FRAMES.length];
  if (!frame) return null;
  const [eyes] = frame;
  return <Text color="white">{eyes}</Text>;
};
