import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { VerbosityPreview } from './VerbosityPreview.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import {
  getVerboseDisplay,
  getTuiVerboseDisplay,
  type VerboseDisplayConfig,
} from '../../../lite/verbose.js';
import { useAppStore } from '../../../stores/app-store.js';

const MAX_CAP = 99999;

// Bigger steps at bigger values so terminal autorepeat (each keypress re-reads
// the value) feels exponential without velocity tracking. Tail step >1000.
const STEPS: ReadonlyArray<readonly [threshold: number, step: number]> = [
  [5, 1],
  [50, 5],
  [200, 25],
  [1000, 100],
];
const TAIL_STEP = 500;

// `<` going up, `<=` going down so the boundary values (5/50/200/1000) step
// down by the smaller band.
function nudge(v: number, dir: 1 | -1): number {
  const step =
    STEPS.find(([t]) => (dir > 0 ? v < t : v <= t))?.[1] ?? TAIL_STEP;
  return dir > 0 ? Math.min(MAX_CAP, v + step) : Math.max(0, v - step);
}

/** Per-field topology: saved-config cap key, preview fixture, editor heading. */
const FIELD_META = {
  argsLines: {
    configKey: 'argsMaxLines',
    previewKey: 'truncation:args',
    heading: 'Tool args · lines',
  },
  argsChars: {
    configKey: 'argsMaxChars',
    previewKey: 'truncation:args',
    heading: 'Tool args · chars',
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
  const glyphs = useGlyphs();
  const dim = useMemo(() => getColor('secondary'), [getColor]);
  const meta = FIELD_META[which];
  const isLite = useAppStore((state) => state.uiMode === 'lite');
  const currentDisplay = isLite ? getVerboseDisplay() : getTuiVerboseDisplay();

  const [value, setValue] = useState<number | null>(
    () => currentDisplay[meta.configKey]
  );

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

  const valueRef = useRef(value);
  valueRef.current = value;
  // digitMode: true once a digit was typed since open/last-arrow, so "123"
  // appends to 123 instead of replacing to 3. Arrow/backspace/u clear it.
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
      const next = nudge(valueRef.current ?? 5, -1);
      setDraft(next === 0 ? null : next);
      return;
    }
    if (key.rightArrow) {
      // null starts at 5 (the original "5 lines" preset).
      setDraft(valueRef.current == null ? 5 : nudge(valueRef.current, 1));
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

  // Override the edited cap so the preview reflects the in-progress draft.
  const display: VerboseDisplayConfig = {
    ...currentDisplay,
    [meta.configKey]: value,
  };

  return (
    <Box flexDirection="column">
      <Box paddingX={1} flexDirection="column">
        <Text>{meta.heading.replace('·', glyphs.smallDot)}</Text>
        <Box height={1} />
        <Box>
          <Text>{dim('  ')}</Text>
          <Text
            inverse={blink}
          >{` ${glyphs.triangleLeft}  ${valueText}  ${glyphs.triangleRight} `}</Text>
        </Box>
        <Box height={1} />
        <Text>
          {dim(
            `  ${glyphs.arrowLeft}/${glyphs.arrow} adjust ${glyphs.smallDot} digits to set ${glyphs.smallDot} backspace to drop ${glyphs.smallDot} u for unlimited ${glyphs.smallDot} ${glyphs.enter} commit ${glyphs.smallDot} esc back`
          )}
        </Text>
      </Box>
      <VerbosityPreview which={meta.previewKey} displayOverride={display} />
    </Box>
  );
};
