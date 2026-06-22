import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { PreviewFrame } from './PreviewFrame.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useAnimationPaused } from '../../../contexts/AnimationPausedContext.js';
import { renderVerbosityPreview } from '../../../lite/render.js';
import {
  getVerboseConfig,
  getVerboseDisplay,
  type VerboseDisplayConfig,
} from '../../../lite/verbose.js';

const MAX_CAP = 99999;

/**
 * Asymmetric step function for arrow-key adjustment. Bigger steps when the
 * value is bigger so holding the arrow feels exponential without explicit
 * velocity tracking — terminal autorepeat fires repeated keypresses, each
 * one consults the current value to pick its own step size.
 */
function nextUp(v: number): number {
  let n: number;
  if (v < 5) n = v + 1;
  else if (v < 50) n = v + 5;
  else if (v < 200) n = v + 25;
  else if (v < 1000) n = v + 100;
  else n = v + 500;
  return Math.min(MAX_CAP, n);
}

function nextDown(v: number): number {
  if (v <= 5) return Math.max(0, v - 1);
  if (v <= 50) return v - 5;
  if (v <= 200) return v - 25;
  if (v <= 1000) return v - 100;
  return v - 500;
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
  // True once the user has typed a digit since open/last-arrow. Controls
  // append-vs-replace on the next digit so typing "123" produces 123, not 3.
  const [digitMode, setDigitMode] = useState(false);

  // Blink for the value chevron — drives a 500ms toggle. Same trick as the
  // Menu search input cursor. Honor /settings allowAnimations: when paused,
  // hold the chevron steady-on rather than running the interval.
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

  // Stash latest committed-on-Enter so the closure passed to useKeypress
  // (which captures state by ref) sees the current value without resubscribing
  // every render.
  const valueRef = useRef(value);
  valueRef.current = value;
  const digitModeRef = useRef(digitMode);
  digitModeRef.current = digitMode;

  // Set value + digitMode together; arrow/backspace/u clear digitMode, digit
  // input sets it (controls append-vs-replace on the next digit).
  const setDraft = (next: number | null, digit = false) => {
    setValue(next);
    setDigitMode(digit);
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
    // collapses to `null` so the user can type a full unlimited from any
    // state without arrowing down through every step.
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
  // draft, not the saved value.
  const display = useMemo(
    (): VerboseDisplayConfig => ({
      ...getVerboseDisplay(),
      [FIELD_META[which].configKey]: value,
    }),
    [which, value]
  );

  const filters = useMemo(() => getVerboseConfig().filters, []);

  const previewKey = FIELD_META[which].previewKey;
  const previewText = useMemo(
    () => renderVerbosityPreview(previewKey, display, filters),
    [previewKey, display, filters]
  );

  const heading = FIELD_META[which].heading;

  return (
    <Box flexDirection="column">
      <Box paddingX={1} flexDirection="column">
        <Text>{heading}</Text>
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
      <PreviewFrame>
        <Text>{previewText}</Text>
      </PreviewFrame>
    </Box>
  );
};
