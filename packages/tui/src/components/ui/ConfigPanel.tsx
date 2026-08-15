/**
 * /config overlay — consolidated config category view. Top screen is a
 * `Category | Source | Status` table; Enter drills into a category page, or
 * hands off to the shared /mcp and /hooks panels so those categories have
 * exactly one view each.
 *
 * This component owns cursor state, key handling, and the store handoffs;
 * row/page/footer content and enter-routing are imported pure functions.
 * Dark-shipped behind `Feature.CloudConfig` — the `/config` command is only
 * registered when the flag is on.
 */

import React, { useMemo, useState } from 'react';
import { useInput, Box } from '../../renderer.js';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import { visibleWidth } from '../../utils/text-width.js';
import {
  type ConfigCategoryId,
  type ConfigSnapshot,
  buildCategoryRows,
  buildCategoryPage,
  buildTopFooterLines,
  resolveCategorySelect,
  CONFIG_HANDOFF_LOADERS,
} from './config-panel-model.js';
import { useAppStore } from '../../stores/app-store.js';
import { recordTuiConfigPanel } from '../../utils/tui-telemetry-observer.js';
import { getCliVersion } from '../../utils/version.js';

interface ConfigPanelProps {
  snapshot: ConfigSnapshot;
  /** Category page to open directly (from `/config <sub>`), if any. */
  initialCategory?: ConfigCategoryId;
  onClose: () => void;
  /** Open the shared /mcp panel (same view as the /mcp command). */
  onOpenMcp: () => void;
  /** Open the existing /hooks panel. */
  onOpenHooks: () => void;
  /** Open the selectable /agent picker (same view as /agent). */
  onOpenAgent: () => void;
}

const GAP = 2;

export const ConfigPanel: React.FC<ConfigPanelProps> = ({
  snapshot,
  initialCategory,
  onClose,
  onOpenMcp,
  onOpenHooks,
  onOpenAgent,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const accent = getColor('accent');
  const brand = getColor('brand');

  const [category, setCategory] = useState<ConfigCategoryId | null>(
    initialCategory ?? null
  );
  const [cursorIndex, setCursorIndex] = useState(0);
  const [search, setSearch] = useState('');
  const [scrollOffset, setScrollOffset] = useState(0);
  // Latched at mount: the Source-column decision is stable for the panel's
  // lifetime. A descriptor push landing mid-panel must not insert the
  // column under the user (rows shifting right, footers gaining lines) —
  // the same stability rule that makes the gate snapshot-wide rather than
  // per-category. Reopening the panel picks up the new facts.
  const [sourcesReported] = useState(snapshot.sourcesReported);
  const stableSnapshot = useMemo(
    () => ({ ...snapshot, sourcesReported }),
    [snapshot, sourcesReported]
  );
  // True while a routed handoff (mcp/hooks) is awaiting its V2 RPC. The
  // panel stays mounted through that window (so the queue stays paused),
  // and a nonzero token makes it inert: a key-repeat Enter must not fire a
  // second RPC / telemetry count. Store-owned monotonic token (not a boolean)
  // so the awaited handler compares by identity — an ESC-cancel or a
  // superseding second handoff is distinguishable from its own completion.
  const handoffInFlight = useAppStore((s) => s.configHandoffToken !== 0);
  const beginHandoff = useAppStore((s) => s.beginConfigHandoff);
  const endHandoff = useAppStore((s) => s.endConfigHandoff);
  // ESC-cancel must also release the RPC-window input lock the handler
  // claimed, or the prompt row stays frozen for the length of the hang the
  // cancel exists to escape.
  const loadingMessage = useAppStore((s) => s.loadingMessage);
  const setLoadingMessage = useAppStore((s) => s.setLoadingMessage);
  // Rows visible at once on a category page: terminal height minus the
  // panel chrome (title, divider, search, column header, footer lines,
  // hints).
  const maxVisible = Math.max(termHeight - 12, 5);

  const categoryRows = useMemo(
    () => buildCategoryRows(stableSnapshot),
    [stableSnapshot]
  );
  const page = useMemo(
    () => (category ? buildCategoryPage(category, stableSnapshot) : null),
    [category, stableSnapshot]
  );

  // Category page rows, search-filtered.
  const filteredPageRows = useMemo(() => {
    if (!page) return [];
    if (!search) return page.rows;
    const q = search.toLowerCase();
    return page.rows
      .map((r) => ({
        r,
        score: r.reduce(
          (max, cell) => Math.max(max, fuzzyScore(q, cell.toLowerCase())),
          0
        ),
      }))
      .filter(({ score }) => score > 0)
      .sort((a, b) => b.score - a.score)
      .map(({ r }) => r);
  }, [page, search]);

  const select = (id: ConfigCategoryId) => {
    const result = resolveCategorySelect(id);
    switch (result.kind) {
      case 'open-mcp':
      case 'open-hooks':
      case 'open-agent':
        // Hand off WITHOUT closing first: the subcommand handler closes
        // /config only after the routed panel is open, so there is never a
        // no-panel window where the paused message queue could drain (or
        // the prompt row grab input) mid-navigation while the V2 RPC is in
        // flight.
        beginHandoff();
        if (result.kind === 'open-mcp') onOpenMcp();
        else if (result.kind === 'open-hooks') onOpenHooks();
        else onOpenAgent();
        return;
      case 'page':
        // In-panel navigation bypasses the store's setShowConfigPanel, so
        // the category-view counter is emitted here instead.
        // engine omitted: /config is KAS-only, the recorder defaults to v3.
        recordTuiConfigPanel({
          category: result.category,
          version: getCliVersion(),
        });
        setSearch('');
        setCursorIndex(0);
        setScrollOffset(0);
        setCategory(result.category);
        return;
      case 'none':
        return;
    }
  };

  useInput(
    (
      _input: string,
      key: { upArrow: boolean; downArrow: boolean; return: boolean }
    ) => {
      if (handoffInFlight) return; // inert while the routed RPC is in flight
      if (category) return; // pages are read-only lists
      if (key.downArrow) {
        setCursorIndex((prev) => Math.min(prev + 1, categoryRows.length - 1));
        return;
      }
      if (key.upArrow) {
        setCursorIndex((prev) => Math.max(prev - 1, 0));
        return;
      }
      if (key.return) {
        const row = categoryRows[cursorIndex];
        if (row) select(row.id);
      }
    }
  );

  // ESC inside a category page backs out to the category list; Panel's own
  // ESC handling then closes from the top screen.
  const handleClose = () => {
    // ESC during a routed handoff CANCELS it: zero the token (the awaited
    // handler compares by identity and opens nothing) and close /config.
    // Also release the RPC-window loader the handler claimed — it clears
    // that only in its finally when the (possibly hung) RPC settles, and
    // loadingMessage is an input lock, so leaving it set would freeze the
    // prompt row for the length of the very hang this cancel escapes. Clear
    // ONLY a config loader (claim-only discipline): if the handler couldn't
    // claim because a compaction spinner held it, that owner keeps it.
    if (handoffInFlight) {
      endHandoff();
      if (
        loadingMessage != null &&
        CONFIG_HANDOFF_LOADERS.has(loadingMessage)
      ) {
        setLoadingMessage(null);
      }
      onClose();
      return;
    }
    if (category) {
      // Symmetric with reopenConfigMenu's ESC-back from a routed panel:
      // every re-entry into the table counts one menu view.
      recordTuiConfigPanel({
        category: 'menu',
        version: getCliVersion(),
      });
      setCategory(null);
      setSearch('');
      setCursorIndex(0);
      setScrollOffset(0);
      return;
    }
    onClose();
  };

  const handleSearchChange = (s: string) => {
    setSearch(s);
    setScrollOffset(0);
  };

  const colWidth = (values: string[], min: number) =>
    Math.max(
      min,
      values.reduce((max, v) => Math.max(max, visibleWidth(v)), 0)
    ) + GAP;

  if (page) {
    // Column widths derive from the FULL page rows, not the filtered set, so
    // the table doesn't jitter while the user types a search.
    const columns = page.columns.map((label, i) =>
      i < page.columns.length - 1
        ? {
            label,
            width: colWidth([label, ...page.rows.map((r) => r[i] ?? '')], 8),
          }
        : { label }
    );
    const canScrollDown = scrollOffset + maxVisible < filteredPageRows.length;
    const visibleRows = filteredPageRows.slice(
      scrollOffset,
      scrollOffset + maxVisible
    );
    const rows: Row[] = visibleRows.map((r) =>
      r.map((text, i) => ({ text, color: i === 0 ? primary : dim }))
    );
    return (
      <Panel
        title={page.title}
        onClose={handleClose}
        searchable
        onSearchChange={handleSearchChange}
        canScrollUp={scrollOffset > 0}
        canScrollDown={canScrollDown}
        onScrollUp={() => setScrollOffset((p) => Math.max(0, p - 1))}
        onScrollDown={() =>
          setScrollOffset((p) =>
            Math.min(Math.max(0, filteredPageRows.length - maxVisible), p + 1)
          )
        }
        // One Escape hint, owned by Panel: "esc to go back" normally,
        // "esc to clear search" while a search is active (Panel swaps the
        // label itself) — a duplicate footer hint would contradict it.
        closeHintLabel="to go back"
      >
        {filteredPageRows.length === 0 ? (
          <Text>{dim(search ? 'No matches' : (page.emptyMessage ?? ''))}</Text>
        ) : (
          <Table columns={columns} rows={rows} />
        )}
        <Box flexDirection="column" marginTop={1}>
          {page.footerLines.map((line, i) => (
            <Text key={i}>{dim(line)}</Text>
          ))}
        </Box>
      </Panel>
    );
  }

  // Source column only when the data carries an origin fact or the session
  // is cloud. A descriptor-free local session hides it rather than showing
  // a placement guess.
  const showSource = sourcesReported;
  const columns = [
    {
      // Chevron marker column + Category. Width covers "❯ " on any row.
      label: '  Category',
      width: colWidth(
        categoryRows.map((r) => `  ${r.label}`),
        14
      ),
    },
    ...(showSource
      ? [
          {
            label: 'Source',
            width: colWidth(
              categoryRows.map((r) => r.source),
              12
            ),
          },
        ]
      : []),
    { label: 'Status' },
  ];
  // The cursor row leads with an accent chevron and accent-colored category
  // instead of inverse-video.
  const rows: Row[] = categoryRows.map((r, i) => {
    const isSel = i === cursorIndex;
    return [
      {
        text: `${isSel ? glyphs.chevron : ' '} ${r.label}`,
        color: isSel ? accent : primary,
      },
      ...(showSource ? [{ text: r.source, color: dim }] : []),
      { text: r.status, color: dim },
    ];
  });

  // Color only the settings URL; keep the surrounding prose dim.
  const renderFooterLine = (line: string, i: number) => {
    const url = 'https://app.kiro.dev/settings';
    const at = line.indexOf(url);
    if (at === -1) return <Text key={i}>{dim(line)}</Text>;
    return (
      <Text key={i}>
        {dim(line.slice(0, at))}
        {brand(url)}
        {dim(line.slice(at + url.length))}
      </Text>
    );
  };

  return (
    <Panel
      title="/config"
      onClose={handleClose}
      searchable={false}
      // One left-aligned hint strip: `esc to close · ↑↓ to navigate · ↵ to
      // select`.
      footerLeft={
        <Text>
          {primary(`${glyphs.arrowUp}${glyphs.arrowDown}`)} {dim('to navigate')}{' '}
          {dim(glyphs.smallDot)} {primary(glyphs.enter)} {dim('to select')}
        </Text>
      }
    >
      <Table columns={columns} rows={rows} />
      <Box flexDirection="column" marginTop={1}>
        {buildTopFooterLines(snapshot.cloudSession, snapshot.diagnostics, {
          cross: glyphs.cross,
          warning: glyphs.warning,
        }).map(renderFooterLine)}
      </Box>
    </Panel>
  );
};
