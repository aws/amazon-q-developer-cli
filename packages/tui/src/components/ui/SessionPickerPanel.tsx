/** Cloud-aware session picker for `/sessions` (controlled view).
 *  Columnar resume table: ID | Name | Environment | Status | Last
 *  updated. Rows span the local and cloud stores; enter resumes the highlighted
 *  session. Purely local listings still render every column (Environment reads
 *  `local`), matching the mock's mixed table. */
import React, { useEffect, useMemo, useState } from 'react';
import { Box, useInput } from '../../renderer.js';
import { Text } from './text/Text.js';
import { Panel } from './panel/Panel.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import {
  padToWidth,
  truncateToWidth,
  visibleWidth,
} from '../../utils/text-width.js';
import { chalk } from '../../utils/color.js';
import { sanitizeSessionTitleForDisplay } from '../../utils/sanitize-title.js';
import { formatRelativeTimeShort } from '../../utils/sessions.js';
import { CLOUD_SESSIONS_URL } from '../../utils/cloud-urls.js';

/** One row the panel renders — the display projection of a merged session entry. */
export interface SessionPickerRow {
  /** Full session id; the short form (first 8 chars) is shown in the ID column. */
  sessionId: string;
  title: string;
  /** `'cloud'` when the session runs on a cloud sandbox, else `'local'`. */
  environment: 'local' | 'cloud';
  /** Display status (e.g. `idle`, `working`, `input needed`), or '' if unknown. */
  status: string;
  /** RFC3339 timestamp; rendered as an abbreviated age (e.g. `12d`, `30w`). */
  updatedAt: string;
}

export interface SessionPickerPanelProps {
  rows: SessionPickerRow[];
  /** Called with the chosen session id when the user presses enter. */
  onSelect: (sessionId: string, environment: 'local' | 'cloud') => void;
  onClose: () => void;
  /** Panel title — the command the user typed (`/chat` or `/sessions`). */
  title?: string;
}

// Upper bound on rows shown at once; the actual window shrinks on short
// terminals (below) so the table never overflows the viewport.
const MAX_VISIBLE_ROWS = 12;

export const SessionPickerPanel: React.FC<SessionPickerPanelProps> = ({
  rows,
  onSelect,
  onClose,
  title = '/sessions',
}) => {
  const { colors, getColor } = useTheme();
  const glyphs = useGlyphs();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const secondaryHex =
    (colors as { secondary?: { truecolor?: string } }).secondary?.truecolor ??
    '#808080';
  const accentHex =
    (colors as { accent?: { truecolor?: string } }).accent?.truecolor ??
    '#ff00ff';
  const brand = getColor('brand');

  const [cursor, setCursor] = useState(0);
  const [search, setSearch] = useState('');

  const filtered = useMemo(() => {
    const q = search.trim().toLowerCase();
    if (!q) return rows;
    return rows.filter(
      (r) =>
        r.title.toLowerCase().includes(q) ||
        r.sessionId.toLowerCase().includes(q) ||
        r.environment.includes(q)
    );
  }, [rows, search]);

  useEffect(() => {
    if (cursor >= filtered.length) setCursor(Math.max(0, filtered.length - 1));
  }, [filtered.length, cursor]);

  useInput((input, key) => {
    if (key.escape) {
      onClose();
      return;
    }
    if (key.return) {
      const row = filtered[cursor];
      if (row) {
        onSelect(row.sessionId, row.environment);
        onClose();
      }
      return;
    }
    if (key.upArrow) {
      setCursor((i) => Math.max(0, i - 1));
      return;
    }
    if (key.downArrow) {
      setCursor((i) => Math.min(filtered.length - 1, i + 1));
      return;
    }
    if (key.backspace || key.delete) {
      setSearch((s) => s.slice(0, -1));
      return;
    }
    if (input && input.length === 1 && input > ' ' && !key.ctrl && !key.meta) {
      setSearch((s) => s + input);
    }
  });

  // Column widths. ID is the fixed 8-char short id (long enough to stay
  // unambiguous and to match what other resume surfaces display); Name flexes;
  // Environment and Status size to their contents; Last updated is compact.
  const idWidth = 8;
  const envWidth = Math.max(
    visibleWidth('Environment'),
    filtered.reduce((w, r) => Math.max(w, visibleWidth(r.environment)), 0)
  );
  const statusWidth = Math.max(
    visibleWidth('Status'),
    filtered.reduce((w, r) => Math.max(w, visibleWidth(r.status)), 0)
  );
  const lastWidth = Math.max(
    visibleWidth('Last updated'),
    filtered.reduce(
      (w, r) => Math.max(w, visibleWidth(formatRelativeTimeShort(r.updatedAt))),
      0
    )
  );
  const prefixWidth = 2; // "> "
  const gap = 3;
  const gapStr = ' '.repeat(gap);
  // Fixed columns + the panel's own left/right padding (paddingX={1} on the
  // panel box, applied to both the border frame and the inner column box = 4
  // cells). Reserve them so the full row — including "Last updated" — fits on
  // one line instead of wrapping.
  const chrome = 4;
  const fixedCols =
    prefixWidth +
    idWidth +
    gap +
    gap +
    envWidth +
    gap +
    statusWidth +
    gap +
    lastWidth +
    chrome;
  const nameWidth = Math.max(termWidth - fixedCols, 12);

  const visibleRows = Math.max(Math.min(termHeight - 9, MAX_VISIBLE_ROWS), 3);
  const start = Math.max(
    0,
    Math.min(
      cursor - Math.floor(visibleRows / 2),
      filtered.length - visibleRows
    )
  );
  const end = Math.min(start + visibleRows, filtered.length);
  const windowRows = filtered.slice(start, end);

  const hint = (k: string, label: string) => (
    <>
      {k} {chalk.hex(secondaryHex)(label)}
    </>
  );

  const col = (text: string, width: number, cursorRow: boolean) => {
    const padded = padToWidth(truncateToWidth(text, width), width);
    return cursorRow
      ? chalk.hex(accentHex)(padded)
      : chalk.hex(secondaryHex)(padded);
  };

  // `(+N more)`: rows scrolled out of the visible window. The listing is uncapped
  // (it carries every session for the cwd), so this counts only what the
  // fixed-height window hides, not a backend truncation.
  const moreCount = Math.max(0, filtered.length - windowRows.length);

  return (
    <Panel
      title={brand(title)}
      onClose={onClose}
      closeHintLabel="to cancel"
      footerLeft={
        <Text>
          {hint(`${glyphs.arrowUp}${glyphs.arrowDown}`, 'to navigate')}
          {chalk.hex(secondaryHex)(` ${glyphs.smallDot} `)}
          {hint(`${glyphs.enter ?? '↵'}`, 'to resume')}
        </Text>
      }
    >
      <Box flexDirection="column">
        {/* Single-Text rows: flex spacer Boxes add stray blank lines in the
            terminal layout, so columns are padded into one string per row. */}
        <Text>
          {chalk.hex(secondaryHex)(
            `  ${padToWidth('ID', idWidth)}${gapStr}${padToWidth('Name', nameWidth)}${gapStr}${padToWidth('Environment', envWidth)}${gapStr}${padToWidth('Status', statusWidth)}${gapStr}Last updated`
          )}
        </Text>

        {windowRows.map((row, wi) => {
          const idx = start + wi;
          const isCursor = idx === cursor;
          const cursorGlyph = isCursor
            ? chalk.hex(accentHex).bold(`${glyphs.chevron}`)
            : ' ';
          const shortId = row.sessionId.slice(0, idWidth);
          const rowTitle =
            sanitizeSessionTitleForDisplay(row.title) || '(no title)';
          const age = formatRelativeTimeShort(row.updatedAt);
          return (
            <Text key={row.sessionId}>
              {cursorGlyph} {col(shortId, idWidth, isCursor)}
              {gapStr}
              {col(rowTitle, nameWidth, isCursor)}
              {gapStr}
              {col(row.environment, envWidth, isCursor)}
              {gapStr}
              {col(row.status, statusWidth, isCursor)}
              {gapStr}
              {col(age, lastWidth, isCursor)}
            </Text>
          );
        })}

        {moreCount > 0 && (
          <Text>{chalk.hex(secondaryHex)(`  (+${moreCount} more)`)}</Text>
        )}
        <Box>
          <Text>{'  '}</Text>
          {search ? <Text>{chalk.hex(accentHex)(search)}</Text> : null}
          <Text inverse> </Text>
          {!search && <Text>{chalk.hex(secondaryHex)(' type to search')}</Text>}
        </Box>
        <Text>
          {chalk.hex(secondaryHex)('  View all cloud sessions at ')}
          {chalk.hex(accentHex)(CLOUD_SESSIONS_URL)}
        </Text>

        {filtered.length === 0 && (
          <Text>{chalk.hex(secondaryHex)('  No sessions found.')}</Text>
        )}
      </Box>
    </Panel>
  );
};
