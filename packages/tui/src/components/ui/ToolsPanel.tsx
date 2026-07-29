import React, { useState, useCallback, useMemo } from 'react';
import { Box } from '../../renderer.js';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { useGlyphs, useAllowIcons } from '../../hooks/useGlyphs.js';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import type {
  ToolInfo,
  ToolStatus,
  InitError,
} from '../../stores/app-store.js';
import { visibleWidth, truncateToWidth } from '../../utils/text-width.js';
import { webToolsGovernanceMessage } from './toolsPanelMessages.js';
import {
  cloudPanelNotice,
  cloudPanelEmptyMessage,
  cloudNoticeLineCount,
} from './cloud-panel-notice.js';
import type { CloudSnapshotReadiness } from '../../stores/app-store.js';

interface ToolsPanelProps {
  tools: ToolInfo[];
  initErrors?: InitError[];
  cloudSessionActive?: boolean;
  /** Sandbox snapshot readiness for this surface (cloud sessions only). */
  cloudSnapshotReadiness?: CloudSnapshotReadiness;
  onClose: () => void;
}

function shortDescription(desc: string, maxLen: number): string {
  const firstLine = desc.trim().split('\n')[0] ?? '';
  const firstSentence = firstLine.split('. ')[0] ?? firstLine;
  const clean = firstSentence.replace(/\s+/g, ' ').trim();
  if (visibleWidth(clean) <= maxLen) return clean;
  return truncateToWidth(clean, maxLen, '...');
}

const GAP = 2;

export const ToolsPanel: React.FC<ToolsPanelProps> = ({
  tools,
  initErrors = [],
  cloudSessionActive = false,
  cloudSnapshotReadiness,
  onClose,
}) => {
  const { getColor } = useTheme();
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const brand = getColor('brand');
  const info = getColor('info');
  const success = getColor('success');
  const warning = getColor('warning');
  const error = getColor('error');

  const statusLabels: Record<ToolStatus, string> = useMemo(
    () => ({
      allowed: `${!allowIcons ? '' : glyphs.dotFilled} allowed`,
      'requires-approval': `${!allowIcons ? '' : glyphs.dotLoading} approval required`,
      denied: `${!allowIcons ? '' : glyphs.cross} denied`,
    }),
    [glyphs, allowIcons]
  );

  // KAS exposes tags with no per-tool status; hide the Status column entirely
  // when no row carries a status (the Rust engine always sets one).
  const showStatus = tools.some((t) => t.status !== undefined);

  const cloudNotice = cloudPanelNotice(
    'tools',
    cloudSessionActive,
    cloudSnapshotReadiness
  );
  const awaitingSandbox =
    cloudSessionActive && cloudSnapshotReadiness === 'awaiting-sandbox';
  const noticeLines = cloudNoticeLineCount(
    cloudNotice,
    termWidth - 2,
    !awaitingSandbox && tools.length > 0
  );
  const maxVisible = Math.max(termHeight - 9 - noticeLines, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const sorted = [...tools].sort((a, b) => {
    if (a.source === 'built-in' && b.source !== 'built-in') return -1;
    if (b.source === 'built-in' && a.source !== 'built-in') return 1;
    return a.source.localeCompare(b.source) || a.name.localeCompare(b.name);
  });

  const q = search.toLowerCase();
  const filtered = search
    ? sorted
        .map((t) => ({
          t,
          score: Math.max(
            fuzzyScore(q, t.name.toLowerCase()),
            fuzzyScore(q, t.source.toLowerCase()),
            fuzzyScore(q, t.description.toLowerCase())
          ),
        }))
        .filter(({ score }) => score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ t }) => t)
    : sorted;

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  const maxNameLen = tools.reduce(
    (max, t) => Math.max(max, visibleWidth(t.name)),
    0
  );
  const nameCol = Math.max(maxNameLen, 12) + GAP;
  const maxSourceLen = tools.reduce(
    (max, t) => Math.max(max, visibleWidth(t.source)),
    0
  );
  const sourceCol = Math.max(maxSourceLen, 10) + GAP;
  const statusCol = showStatus ? 20 + GAP : 0;
  const descCol = Math.max(termWidth - nameCol - sourceCol - statusCol - 2, 10);

  const statusColor = (status: ToolStatus) => {
    switch (status) {
      case 'allowed':
        return success;
      case 'requires-approval':
        return warning;
      case 'denied':
        return error;
    }
  };
  const sourceColor = (source: string) =>
    source === 'built-in' || source === 'builtin' ? brand : info;

  const columns = [
    { label: 'Name', width: nameCol },
    { label: 'Source', width: sourceCol },
    ...(showStatus ? [{ label: 'Status', width: statusCol }] : []),
    { label: 'Description' },
  ];

  const rows: Row[] = useMemo(
    () =>
      visible.map((tool) => {
        const cells: Row = [
          { text: tool.name, color: primary },
          { text: tool.source, color: sourceColor(tool.source) },
        ];
        if (showStatus) {
          const st = tool.status ?? 'requires-approval';
          cells.push({ text: statusLabels[st], color: statusColor(st) });
        }
        cells.push({
          text: shortDescription(tool.description, descCol),
          color: dim,
        });
        return cells;
      }),
    [
      visible,
      descCol,
      showStatus,
      primary,
      dim,
      brand,
      info,
      success,
      warning,
      error,
      statusLabels,
    ]
  );

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  const governanceMessage = webToolsGovernanceMessage(initErrors);
  const governanceWarning = governanceMessage
    ? `${!allowIcons ? '' : `${glyphs.warning} `}${governanceMessage}`
    : undefined;

  // The Rust engine lists individual tools (with status); KAS lists tags
  // (no status). Reflect that in the title noun.
  const noun = showStatus ? 'tool' : 'tag';
  const title = `/tools ${glyphs.smallDot} ${tools.length} ${noun}${tools.length === 1 ? '' : 's'}`;

  return (
    <Panel
      title={title}
      onClose={onClose}
      searchable={true}
      onSearchChange={handleSearchChange}
      canScrollUp={scrollOffset > 0}
      canScrollDown={canScrollDown}
      onScrollUp={() => setScrollOffset((p) => Math.max(0, p - 1))}
      onScrollDown={() =>
        setScrollOffset((p) =>
          Math.min(Math.max(0, filtered.length - maxVisible), p + 1)
        )
      }
    >
      <Box flexDirection="column">
        {cloudNotice && (
          <Box
            marginBottom={
              !awaitingSandbox && (tools.length > 0 || governanceWarning)
                ? 1
                : 0
            }
          >
            <Text>{info(cloudNotice)}</Text>
          </Box>
        )}
        {awaitingSandbox ? null : (
          <>
            {governanceWarning && (
              <Box marginBottom={tools.length > 0 ? 1 : 0}>
                <Text>{warning(governanceWarning)}</Text>
              </Box>
            )}
            {tools.length === 0 ? (
              <Text>
                {dim(
                  cloudSessionActive
                    ? cloudPanelEmptyMessage('tools')
                    : 'No tools available'
                )}
              </Text>
            ) : (
              <Table columns={columns} rows={rows} />
            )}
          </>
        )}
      </Box>
    </Panel>
  );
};
