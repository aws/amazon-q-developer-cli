/** Cloud-only multi-select repository picker for `/repo` (controlled view).
 *  Columns: name, Provider, Last used (from the source's `updatedAt`). */
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
import chalk from 'chalk';
import type { SourceProviderResource } from '@kiro/acp-type-covenant';
import {
  filterRepoResources,
  toggleRepoSelection,
  formatRepoLastUsed,
} from '../../utils/repo-multiselect.js';

export interface RepoPickerPanelProps {
  resources: SourceProviderResource[];
  /** Repos already attached to the session; pre-checked when the picker opens. */
  initialSelected?: string[];
  /** Called with the selected repo names when the user saves (esc). */
  onSubmit: (selected: string[]) => void;
  /** Called to dismiss the panel (after submit, or on cancel with no changes). */
  onClose: () => void;
}

/** Cap on list rows; short terminals shrink below this so the panel chrome
 *  (title, search, header, hints — ~9 lines) still fits without overflow. */
const MAX_VISIBLE_ROWS = 10;

export const RepoPickerPanel: React.FC<RepoPickerPanelProps> = ({
  resources,
  initialSelected,
  onSubmit,
  onClose,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const visibleRows = Math.max(Math.min(termHeight - 9, MAX_VISIBLE_ROWS), 3);
  const secondaryHex = getColor('secondary').hex;
  const accentHex = getColor('accent').hex;

  const [cursor, setCursor] = useState(0);
  const [search, setSearch] = useState('');
  // Seed from the session's already-attached repos so reopening /repo shows them
  // checked. Mount-only: later toggles are the user's, not prop-driven.
  const [selected, setSelected] = useState<string[]>(initialSelected ?? []);

  const filtered = useMemo(
    () => filterRepoResources(resources, search),
    [resources, search]
  );

  // Resource objects for the selected names, in selection order, so the Selected
  // panel can show each repo's default branch (a missing match falls back to a
  // name-only row so a stale selection still renders).
  const selectedRows = useMemo(
    () =>
      selected.map(
        (name) =>
          resources.find((r) => r.name === name) ??
          ({ name, providerType: '' } as SourceProviderResource)
      ),
    [selected, resources]
  );

  // Clamp cursor when the filter shrinks the list.
  useEffect(() => {
    if (cursor >= filtered.length) setCursor(Math.max(0, filtered.length - 1));
  }, [filtered.length, cursor]);

  useInput((input, key) => {
    if (key.escape) {
      // esc saves the current selection.
      onSubmit(selected);
      onClose();
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
    if (input === ' ') {
      // space toggles selection rather than typing into the search box.
      const row = filtered[cursor];
      if (row) setSelected((s) => toggleRepoSelection(s, row.name));
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

  const providerWidth = useMemo(
    () =>
      Math.max(
        visibleWidth('Provider'),
        filtered.reduce((w, r) => Math.max(w, visibleWidth(r.providerType)), 0)
      ),
    [filtered]
  );
  const lastUsedWidth = useMemo(
    () =>
      Math.max(
        visibleWidth('Last used'),
        filtered.reduce(
          (w, r) =>
            Math.max(w, visibleWidth(formatRepoLastUsed(r.updatedAt, glyphs))),
          0
        )
      ),
    [filtered, glyphs]
  );
  // Reserve room for the `>[x] ` prefix (cursor + checkbox) and the gaps.
  const prefixWidth = 5;
  const gapWidth = 4;
  const nameWidth = Math.max(
    termWidth -
      prefixWidth -
      gapWidth -
      providerWidth -
      gapWidth -
      lastUsedWidth -
      4,
    20
  );

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

  return (
    <Panel
      title={getColor('brand')('/repo')}
      onClose={onClose}
      closeHintLabel="to save and exit"
      footerLeft={
        <Text>
          {hint(`${glyphs.arrowUp}${glyphs.arrowDown}`, 'to navigate')}
          {chalk.hex(secondaryHex)(` ${glyphs.smallDot} `)}
          {hint('space', 'to toggle')}
        </Text>
      }
    >
      <Box flexDirection="column">
        <Box flexDirection="row" justifyContent="space-between">
          <Text>{chalk.hex(secondaryHex)(`Selected(${selected.length})`)}</Text>
          <Text>{hint('tab', 'to switch panels')}</Text>
        </Box>
        {/* Each chosen repo with its default branch, so the user sees exactly
            what will be cloned. The row is accented while its cursor is on it. */}
        {selectedRows.map((row) => {
          const branch = row.defaultBranch?.trim();
          const label = branch ? `${row.name} ${branch}` : row.name;
          const isCursorRow = filtered[cursor]?.name === row.name;
          const styled = isCursorRow
            ? chalk
                .hex(accentHex)
                .bold(`${glyphs.chevron}[${glyphs.checkmark}] ${label}`)
            : ` [${glyphs.checkmark}] ${label}`;
          return (
            <Text key={`sel:${row.providerType}:${row.name}`}>{styled}</Text>
          );
        })}
        <Box height={1} />

        <Box marginBottom={1}>
          {search ? (
            <Text>{chalk.hex(getColor('brand').hex)(search)}</Text>
          ) : null}
          <Text inverse> </Text>
          {!search && <Text>{chalk.hex(secondaryHex)(' type to search')}</Text>}
        </Box>

        <Box flexDirection="row">
          <Text>
            {padToWidth(`All(${filtered.length})`, prefixWidth + nameWidth)}
          </Text>
          <Box width={gapWidth} />
          <Text>
            {chalk.hex(secondaryHex)(padToWidth('Provider', providerWidth))}
          </Text>
          <Box width={gapWidth} />
          <Text>{chalk.hex(secondaryHex)('Last used')}</Text>
        </Box>

        {windowRows.map((row, wi) => {
          const idx = start + wi;
          const isCursor = idx === cursor;
          const isChecked = selected.includes(row.name);
          const cursorGlyph = isCursor
            ? chalk.hex(accentHex).bold(`${glyphs.chevron}`)
            : ' ';
          const checkbox = `[${isChecked ? glyphs.checkmark : ' '}]`;
          const name = truncateToWidth(row.name, nameWidth);
          const nameStyled = isCursor
            ? chalk.hex(accentHex).bold(padToWidth(name, nameWidth))
            : padToWidth(name, nameWidth);
          const providerStyled = isCursor
            ? chalk.hex(accentHex)(padToWidth(row.providerType, providerWidth))
            : chalk.hex(secondaryHex)(
                padToWidth(row.providerType, providerWidth)
              );
          const lastUsed = formatRepoLastUsed(row.updatedAt, glyphs);
          const lastUsedStyled = isCursor
            ? chalk.hex(accentHex)(lastUsed)
            : chalk.hex(secondaryHex)(lastUsed);
          return (
            <Box key={`${row.providerType}:${row.name}`} flexDirection="row">
              <Text>
                {cursorGlyph}
                {checkbox} {nameStyled}
              </Text>
              <Box width={gapWidth} />
              <Text>{providerStyled}</Text>
              <Box width={gapWidth} />
              <Text>{lastUsedStyled}</Text>
            </Box>
          );
        })}

        {end < filtered.length && (
          <Box>
            <Text>
              {chalk.hex(secondaryHex)(`  (+${filtered.length - end} more)`)}
            </Text>
          </Box>
        )}

        {filtered.length === 0 && (
          <Box>
            <Text>
              {chalk.hex(secondaryHex)('  No matching repositories.')}
            </Text>
          </Box>
        )}
      </Box>
    </Panel>
  );
};
