import React, { useState, useCallback, useMemo } from 'react';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { fuzzyScore } from '../../utils/fuzzyScore.js';

export interface RequestStat {
  request_id: string | null;
  timestamp: string;
  duration_ms: number | null;
  ttfc_ms: number | null;
  input_tokens: number | null;
  output_tokens: number | null;
  status_code: number | null;
  had_tool_use: boolean;
  error: string | null;
}

interface StatsPanelProps {
  stats: RequestStat[];
  summary: {
    avg_ms: number;
    p90_ms: number;
    max_ms: number;
    errors: number;
  } | null;
  onClose: () => void;
}

function fmtMs(v: number | null): string {
  return v != null ? `${Math.round(v)}ms` : '-';
}

const GAP = 2;

export const StatsPanel: React.FC<StatsPanelProps> = ({
  stats,
  summary,
  onClose,
}) => {
  const { getColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const error = getColor('error');
  const success = getColor('success');
  const warning = getColor('warning');

  const maxVisible = Math.max(termHeight - 11, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const q = search.toLowerCase();
  const filtered = search
    ? stats
        .map((s, i) => ({
          s,
          i,
          score: Math.max(
            fuzzyScore(q, (s.request_id ?? '').toLowerCase()),
            fuzzyScore(q, (s.error ?? '').toLowerCase())
          ),
        }))
        .filter(({ score }) => score > 0)
        .map(({ s, i }) => ({ s, i }))
    : stats.map((s, i) => ({ s, i }));

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  const columns = [
    { label: '#', width: 5 },
    { label: 'Request ID', width: 40 },
    { label: 'Duration', width: 10 + GAP },
    { label: 'TTFC', width: 8 + GAP },
    { label: 'In', width: 8 + GAP },
    { label: 'Out', width: 8 + GAP },
    { label: 'Status' },
  ];

  const rows: Row[] = useMemo(
    () =>
      visible.map(({ s, i }) => {
        const statusColor = s.error
          ? error
          : s.had_tool_use
            ? warning
            : success;
        const statusText = s.error
          ? `ERR: ${s.error.slice(0, 40)}`
          : s.had_tool_use
            ? 'ok (tool_use)'
            : 'ok';
        return [
          { text: String(i + 1), color: dim },
          { text: s.request_id ?? '-', color: primary },
          {
            text: fmtMs(s.duration_ms),
            color:
              s.duration_ms != null && s.duration_ms > 5000 ? warning : dim,
          },
          { text: fmtMs(s.ttfc_ms), color: dim },
          {
            text: s.input_tokens != null ? String(s.input_tokens) : '-',
            color: dim,
          },
          {
            text: s.output_tokens != null ? String(s.output_tokens) : '-',
            color: dim,
          },
          { text: statusText, color: statusColor },
        ];
      }),
    [visible, primary, dim, error, success, warning]
  );

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  const summaryText = summary
    ? `avg=${Math.round(summary.avg_ms)}ms  p90=${Math.round(summary.p90_ms)}ms  max=${Math.round(summary.max_ms)}ms  errors=${summary.errors}`
    : '';

  return (
    <Panel
      title={`/stats · ${stats.length} request${stats.length === 1 ? '' : 's'}`}
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
      footerLeft={summary ? <Text>{dim(summaryText)}</Text> : undefined}
    >
      {stats.length === 0 ? (
        <Text>{dim('No requests recorded yet')}</Text>
      ) : (
        <Table columns={columns} rows={rows} />
      )}
    </Panel>
  );
};
