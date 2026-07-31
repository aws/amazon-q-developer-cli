/**
 * `/settings – display – status line`: choose which segments the bar shows.
 *
 * Rows are grouped by whether a segment is on by default for the surface being
 * edited, because the two surfaces ship different defaults. Anything conditional
 * about when a segment has a value is stated in its description, so a group name
 * never contradicts the on/off column.
 *
 * The edited surface arrives as a prop rather than being read here, which keeps
 * the panel usable from either layout without it deciding which one it is in.
 */
import React, { useCallback, useMemo, useState } from 'react';
import { Box, CURSOR_MARKER, Text, useInput } from '../../renderer.js';
import { Panel } from './panel/Panel.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTextStyle } from '../../hooks/useTextStyle.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useAppStore } from '../../stores/app-store.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import type { UiMode } from '../../types/ui-mode.js';
import {
  defaultStatusSegments,
  resetStatusSegments,
  toggleStatusSegment,
} from '../layout/status-line/config.js';
import { useStatusSegments } from '../layout/status-line/useStatusSegments.js';
import {
  STATUS_SEGMENT_GROUPS,
  STATUS_SEGMENT_LABELS,
} from '../layout/status-line/labels.js';
import { statusSegmentIdsFor } from '../layout/status-line/registry.js';
import type { StatusSegmentId } from '../layout/status-line/segments.js';

type Row =
  | { kind: 'group'; label: string }
  | { kind: 'segment'; id: StatusSegmentId }
  | { kind: 'reset' };

/**
 * The start of a row: a chevron on the selected one, blanks otherwise.
 *
 * `CURSOR_MARKER` must share the row's string; as its own node the escape never
 * reaches the terminal. Being zero-width it parks the hardware cursor without
 * shifting the columns, which is how a screen reader follows the selection.
 */
export function rowLead(selected: boolean, chevron: string): string {
  return selected ? `${CURSOR_MARKER}${chevron} ` : '  ';
}

/** Title, dividers, heading, footer and the blank lines around them. */
const CHROME_ROWS = 12;
/** Below this a window is pointless; the pane is too short either way. */
const MIN_VISIBLE_ROWS = 6;
/** Column widths, padded in the string so a row is one Text node. */
const LABEL_COL = 19;
const STATE_COL = 5;

interface StatusLineSettingsPanelProps {
  /** The surface whose list is being edited. */
  surface: UiMode;
  onClose: () => void;
  onDismiss?: () => void;
}

/** Flatten the grouped segments into rows, with group headers interleaved. */
function buildRows(surface: UiMode): Row[] {
  const rows: Row[] = [];
  const defaults = defaultStatusSegments(surface);
  const available = statusSegmentIdsFor(surface);
  for (const group of STATUS_SEGMENT_GROUPS) {
    const ids = available.filter(
      (id) => (defaults[id] ? 'On by default' : 'Off by default') === group
    );
    if (ids.length === 0) continue;
    rows.push({ kind: 'group', label: group });
    for (const id of ids) rows.push({ kind: 'segment', id });
  }
  rows.push({ kind: 'reset' });
  return rows;
}

export const StatusLineSettingsPanel: React.FC<
  StatusLineSettingsPanelProps
> = ({ surface, onClose, onDismiss }) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const label = useTextStyle('label');
  const selectedLabel = useTextStyle('selectedLabel');
  const dimText = getColor('secondary');
  const brandText = getColor('primary');
  const fromSettings = useAppStore((state) => state.settingsReturnOnEscape);

  const rows = useMemo(() => buildRows(surface), [surface]);
  const selectableIndexes = useMemo(
    () =>
      rows.map((r, i) => (r.kind === 'group' ? -1 : i)).filter((i) => i >= 0),
    [rows]
  );
  const [cursor, setCursor] = useState(selectableIndexes[0] ?? 0);
  // Subscribed rather than held locally: a write is async, so a value read back
  // straight after one would still be the old one.
  const visible = useStatusSegments(surface);

  // The list is longer than a short pane, so show a window around the cursor. A
  // heading can scroll out of view; the on/off column carries the state either way.
  // Read reactively: a pane shrunk while this is open would otherwise overflow.
  const { height } = useTerminalSize();
  const window = useMemo(() => {
    // A heading costs two lines because of the gap above it, so budget for both.
    const headingCost = STATUS_SEGMENT_GROUPS.length * 2;
    const rowsVisible = Math.max(
      MIN_VISIBLE_ROWS,
      height - CHROME_ROWS - headingCost
    );
    if (rows.length <= rowsVisible) return { start: 0, end: rows.length };
    const half = Math.floor(rowsVisible / 2);
    const start = Math.min(
      Math.max(0, cursor - half),
      rows.length - rowsVisible
    );
    return { start, end: start + rowsVisible };
  }, [cursor, rows.length, height]);

  const step = useCallback(
    (delta: number) => {
      const at = selectableIndexes.indexOf(cursor);
      const next = Math.min(
        selectableIndexes.length - 1,
        Math.max(0, (at === -1 ? 0 : at) + delta)
      );
      setCursor(selectableIndexes[next] ?? cursor);
    },
    [cursor, selectableIndexes]
  );

  // Reset answers the same keys as a segment: a row ignoring them reads as a
  // broken key, and reaching it by accident only costs re-picking the segments.
  const activate = useCallback(() => {
    const row = rows[cursor];
    if (row?.kind === 'reset') void resetStatusSegments(surface);
    else if (row?.kind === 'segment') void toggleStatusSegment(surface, row.id);
  }, [cursor, rows, surface]);

  useInput((_input, key) => {
    if (key.upArrow) step(-1);
    else if (key.downArrow) step(1);
    else if (key.leftArrow || key.rightArrow) activate();
    else if (key.return) {
      activate();
      (onDismiss ?? onClose)();
    }
  });

  return (
    <Panel
      title="/settings – display – status line"
      onClose={onClose}
      closeHintLabel={fromSettings ? 'to go back' : 'to close'}
      canScrollUp={window.start > 0}
      canScrollDown={window.end < rows.length}
      footerLeft={
        rows[cursor]?.kind === 'reset' ? (
          <Text>
            {brandText(`${glyphs.arrowLeft}${glyphs.arrow}`)}{' '}
            {dimText('to restore the defaults')}
            {dimText(` ${glyphs.smallDot} `)}
            {brandText(glyphs.enter)} {dimText('to restore and close')}
          </Text>
        ) : (
          <Text>
            {brandText(`${glyphs.arrowLeft}${glyphs.arrow}`)}{' '}
            {dimText('to toggle')}
            {dimText(` ${glyphs.smallDot} `)}
            {brandText(glyphs.enter)} {dimText('to toggle and close')}
          </Text>
        )
      }
    >
      <Box flexDirection="column">
        <Box height={1} />
        <Box marginBottom={1}>
          <Text>
            {dimText(`Which segments should the ${surface} status line show?`)}
          </Text>
        </Box>
        {rows.slice(window.start, window.end).map((row, offset) => {
          const i = window.start + offset;
          if (row.kind === 'group') {
            return (
              <Box key={`group-${row.label}`} marginTop={i === 0 ? 0 : 1}>
                <Text>{dimText(row.label)}</Text>
              </Box>
            );
          }
          const active = i === cursor;
          const lead = rowLead(active, glyphs.chevron);
          if (row.kind === 'reset') {
            return (
              <Box key="reset">
                <Text>
                  {(active ? selectedLabel : label)(
                    `${lead}${'Reset to defaults'.padEnd(LABEL_COL + STATE_COL)}`
                  )}
                  {dimText(`Restore the built-in ${surface} bar`)}
                </Text>
              </Box>
            );
          }
          const meta = STATUS_SEGMENT_LABELS[row.id];
          return (
            <Box key={row.id}>
              <Text>
                {(active ? selectedLabel : label)(
                  `${lead}${meta.label.padEnd(LABEL_COL)}`
                )}
                {brandText((visible[row.id] ? 'on' : 'off').padEnd(STATE_COL))}
                {dimText(meta.description)}
              </Text>
            </Box>
          );
        })}
      </Box>
    </Panel>
  );
};
