import React, { useState, useEffect } from 'react';
import { Box } from './../../../renderer.js';
import { Menu } from './Menu.js';
import { AgentDiagnosticsDetails } from './AgentDiagnosticsDetails.js';
import { useAppStore } from '../../../stores/app-store.js';

/** Visible content rows for both the list and detail view. */
const VISIBLE_ITEMS = 10;
/** Chrome lines surrounding content: search/header + spacer + overflow + divider + footer. */
const CHROME_LINES = 5;
const MENU_HEIGHT = VISIBLE_ITEMS + CHROME_LINES;

export interface UpgradeDiagnosticsMenuProps {
  onDismiss: () => void;
}

/**
 * `/upgrade-agent diagnostics` UI. Mirrors PromptsMenu: a searchable `Menu`
 * list of agents that drills into a full-height {@link AgentDiagnosticsDetails}
 * on → / Enter. Rendered via the activeCommand path (see CommandMenu) so the
 * prompt input is suppressed — fixing the old Explorer-overlay focus bug where
 * ↑↓ hijacked prompt history.
 */
export const UpgradeDiagnosticsMenu: React.FC<UpgradeDiagnosticsMenuProps> = ({
  onDismiss,
}) => {
  const rows = useAppStore((s) => s.upgradeAnalysisRows);
  const description = useAppStore((s) => s.upgradeAnalysisDescription);
  const setUpgradeDiagnostics = useAppStore((s) => s.setUpgradeDiagnostics);

  const [detailName, setDetailName] = useState<string | null>(null);
  const detailRow = rows.find((r) => r.name === detailName) ?? null;

  // Mirror detail state into the store so LiteLayout's always-armed Esc skips
  // its panel-close branch while the detail sub-view owns Esc (same reason
  // PromptsMenu sets this flag).
  const setPromptDetailOpen = useAppStore((s) => s.setPromptDetailOpen);
  useEffect(() => {
    setPromptDetailOpen(detailRow != null);
    return () => setPromptDetailOpen(false);
  }, [detailRow, setPromptDetailOpen]);

  // Drop the cached diagnostics rows when the menu closes.
  useEffect(() => () => setUpgradeDiagnostics([], ''), [setUpgradeDiagnostics]);

  const items = rows.map((r) => {
    const n = r.warnings.length;
    const warn = n > 0 ? `${n} warning${n === 1 ? '' : 's'}` : 'no warnings';
    // 3 columns like the picker: name · scope (group) · warnings. All rows are
    // in-sync universal agents, so the classification isn't repeated per row.
    return {
      label: r.name,
      group: r.scope === 'local' ? 'Workspace' : 'Global',
      description: warn,
    };
  });

  return (
    <Box flexDirection="column" height={MENU_HEIGHT} overflow="hidden">
      {detailRow ? (
        <AgentDiagnosticsDetails
          key={detailRow.name}
          row={detailRow}
          visibleLines={VISIBLE_ITEMS}
          onBack={() => setDetailName(null)}
        />
      ) : (
        <Menu
          items={items}
          prefix=""
          onSelect={(item) => setDetailName(item.label)}
          onRightArrow={(item) => setDetailName(item.label)}
          onEscape={onDismiss}
          showSelectedIndicator={true}
          visibleItems={VISIBLE_ITEMS}
          searchable={true}
          searchLabel={description || 'Diagnostics'}
          searchPlaceholder="type to filter agents"
          showFooterHints={true}
        />
      )}
    </Box>
  );
};
