import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { VerbosityPreview } from './VerbosityPreview.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';
import {
  getVerboseDisplay,
  type VerboseDisplayConfig,
} from '../../../lite/verbose.js';

const MAX_CAP = 99999;

// Step ladder: below each threshold, adjust by that step. Bigger steps at
// bigger values so terminal autorepeat (each keypress re-reads the value)
// feels exponential without velocity tracking. Tail step applies above 1000.
const STEPS: ReadonlyArray<readonly [threshold: number, step: number]> = [
  [5, 1],
  [50, 5],
  [200, 25],
  [1000, 100],
];
const TAIL_STEP = 500;

function stepFor(v: number): number {
  return STEPS.find(([t]) => v < t)?.[1] ?? TAIL_STEP;
}

function nextUp(v: number): number {
  return Math.min(MAX_CAP, v + stepFor(v));
}

function nextDown(v: number): number {
  // Match the previous boundaries (`<=` so 5/50/200/1000 step down by the
  // smaller band) and floor at 0.
  const step = STEPS.find(([t]) => v <= t)?.[1] ?? TAIL_STEP;
  return Math.max(0, v - step);
}

/**
 * Per-field topology: the saved-config cap key, the preview fixture, and the
 * editor heading. Single source so the value/display/heading derivations below
 * don't re-encode the field→key mapping by hand.
 */
const FIELD_META = {
  argsLines: {
    configKey: 'argsMaxLines',
    previewKey: 'truncation:args',
    heading: 'Tool args · lines',
  },
  argsChars: {
    configKey: 'argsMaxChars',
    previewKey: 'truncation:args',
    heading: 'Tool args · chars per value',
  },
  outputLines: {
    configKey: 'outputMaxLines',
    previewKey: 'truncation:output',
    heading: 'Tool output · lines',
  },
  outputChars: {
    configKey: 'outputMaxChars',
    previewKey: 'truncation:output',
    heading: 'Tool output · chars per line',
  },
} as const;

export type TruncationEditorField = keyof typeof FIELD_META;
type CapKey = (typeof FIELD_META)[TruncationEditorField]['configKey'];

/** Saved-config cap key for an editor field, for the `set:<key>:<value>` route. */
export function truncationConfigKey(which: TruncationEditorField): CapKey {
  return FIELD_META[which].configKey;
}

export const VerbosityTruncationEditor: React.FC<{
  which: TruncationEditorField;
  onCommit: (value: number | null) => void;
  onCancel: () => void;
}> = ({ which, onCommit, onCancel }) => {
  const { getColor } = useTheme();
  const dim = useMemo(() => getColor('secondary'), [getColor]);

  // Seed from saved config so the editor opens on the current value.
  const initial = useMemo(
    () => getVerboseDisplay()[FIELD_META[which].configKey],
    [which]
  );

  const [value, setValue] = useState<number | null>(initial);

  // Honor /settings allowAnimations: when paused, hold the chevron steady-on.
  const animationPaused = useAnimationPaused();
  const [blink, setBlink] = useState(true);
  useEffect(() => {
    if (animationPaused) {
      setBlink(true);
      return;
    }
    const id = setInterval(() => setBlink((b) => !b), 500);
    return () => clearInterval(id);
  }, [animationPaused]);

  // The useKeypress closure captures state by ref; mirror so it reads current
  // values without resubscribing every render.
  const valueRef = useRef(value);
  valueRef.current = value;
  // digitMode is non-render state (only the next keypress reads it): true once
  // the user typed a digit since open/last-arrow, controlling append-vs-replace
  // so typing "123" produces 123, not 3. Arrow/backspace/u clear it.
  const digitModeRef = useRef(false);

  const setDraft = (next: number | null, digit = false) => {
    setValue(next);
    digitModeRef.current = digit;
  };

  useKeypress((input, key) => {
    if (key.escape) {
      onCancel();
      return;
    }
    if (key.return) {
      onCommit(valueRef.current);
      return;
    }
    if (key.leftArrow) {
      const next = nextDown(valueRef.current ?? 5);
      setDraft(next === 0 ? null : next);
      return;
    }
    if (key.rightArrow) {
      // From null we start at 5 (matches the original "5 lines" preset).
      setDraft(valueRef.current == null ? 5 : nextUp(valueRef.current));
      return;
    }
    if (key.backspace || key.delete) {
      const cur = valueRef.current;
      if (cur == null) return;
      const s = String(cur);
      if (s.length <= 1) {
        setDraft(null);
      } else {
        const next = parseInt(s.slice(0, -1), 10);
        setDraft(Number.isFinite(next) && next > 0 ? next : null, true);
      }
      return;
    }
    if (input === 'u' || input === 'U') {
      setDraft(null);
      return;
    }
    // Digit input — append in digitMode, replace otherwise. `0` alone
    // collapses to `null` (unlimited) without arrowing down through every step.
    if (/^[0-9]$/.test(input)) {
      const d = parseInt(input, 10);
      if (digitModeRef.current && valueRef.current != null) {
        setDraft(Math.min(MAX_CAP, valueRef.current * 10 + d), true);
      } else {
        setDraft(d === 0 ? null : d, true);
      }
      return;
    }
  });

  const valueText = value == null ? 'unlimited' : String(value);

  // Override the cap being edited so the preview reflects the in-progress
  // draft, not the saved value; VerbosityPreview reads saved filters itself.
  const display = useMemo(
    (): VerboseDisplayConfig => ({
      ...getVerboseDisplay(),
      [FIELD_META[which].configKey]: value,
    }),
    [which, value]
  );

  return (
    <Box flexDirection="column">
      <Box paddingX={1} flexDirection="column">
        <Text>{FIELD_META[which].heading}</Text>
        <Box height={1} />
        <Box>
          <Text>{dim('  ')}</Text>
          <Text inverse={blink}>{` ◀  ${valueText}  ▶ `}</Text>
        </Box>
        <Box height={1} />
        <Text>
          {dim(
            '  ←/→ adjust · digits to set · backspace to drop · u for unlimited · ↵ commit · esc back'
          )}
        </Text>
      </Box>
      <VerbosityPreview
        which={FIELD_META[which].previewKey}
        displayOverride={display}
      />
    </Box>
  );
};
