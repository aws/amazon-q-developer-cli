import React, { useState, useCallback } from 'react';
import { Box, useInput } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Panel } from './panel/Panel.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import type { CodePanelData, CodeLspInfo } from '../../stores/app-store.js';

interface CodePanelProps {
  data: CodePanelData | null;
  onClose: () => void;
  onRefresh?: () => void;
}

function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
}

export function CodePanel({ data, onClose, onRefresh }: CodePanelProps) {
  const { getColor } = useTheme();
  const primary = getColor('primary');
  const secondary = getColor('secondary');
  const success = getColor('success');
  const warning = getColor('warning');
  const error = getColor('error');

  useInput((input) => {
    if (input === 'r' && onRefresh) {
      onRefresh();
    }
  });

  if (!data) {
    return (
      <Panel title="/code" onClose={onClose}>
        <Text>{secondary('Loading...')}</Text>
      </Panel>
    );
  }

  // Logs view
  if (data.entries) {
    return <CodeLogsView data={data} onClose={onClose} />;
  }

  // Status view
  const statusIcon =
    data.status === 'initialized'
      ? '✓'
      : data.status === 'initializing'
        ? '◐'
        : '⚠';
  const statusColor =
    data.status === 'initialized'
      ? success
      : data.status === 'initializing'
        ? warning
        : error;

  return (
    <Panel
      title="/code"
      onClose={onClose}
      footerExtra={
        onRefresh && data.status === 'initializing' ? (
          <Text>
            {primary('r')} {secondary('to refresh')}
          </Text>
        ) : undefined
      }
    >
      <Text>
        {statusColor(`${statusIcon} ${data.status}`)}{' '}
        {data.message ? secondary(`— ${data.message}`) : ''}
      </Text>

      {data.warning && (
        <Box marginTop={1}>
          <Text>{warning(`⚠ ${data.warning}`)}</Text>
        </Box>
      )}

      {data.rootPath ? (
        <Box flexDirection="column" marginTop={1}>
          <Text>
            {primary('Workspace: ')}
            {secondary(data.rootPath)}
          </Text>
          {data.detectedLanguages.length > 0 && (
            <Text>
              {primary('Languages: ')}
              {secondary(data.detectedLanguages.join(', '))}
            </Text>
          )}
          {data.projectMarkers.length > 0 && (
            <Text>
              {primary('Markers: ')}
              {secondary(data.projectMarkers.join(', '))}
            </Text>
          )}
        </Box>
      ) : null}

      {data.lsps && data.lsps.length > 0 && (
        <Box flexDirection="column" marginTop={1}>
          <Text>{primary('LSP Servers:')}</Text>
          <Table
            columns={[
              {
                label: 'Server',
                width: Math.max(...data.lsps.map((l) => l.name.length), 8) + 4,
              },
              {
                label: 'Languages',
                width:
                  Math.max(
                    ...data.lsps.map((l) => l.languages.join(', ').length),
                    10
                  ) + 2,
              },
              { label: 'Status' },
            ]}
            rows={data.lsps.map((lsp) =>
              lspRow(lsp, { success, warning, error, secondary })
            )}
            showHeaders={false}
          />
        </Box>
      )}

      <Box marginTop={1}>
        <Text>
          {secondary('Config: ')}
          {primary(data.configPath)}
        </Text>
      </Box>

      {data.docUrl && (
        <Box>
          <Text>
            {secondary('Learn more at ')}
            {primary(data.docUrl)}
          </Text>
        </Box>
      )}
    </Panel>
  );
}

function lspRow(
  lsp: CodeLspInfo,
  colors: { success: any; warning: any; error: any; secondary: any }
): Row {
  const icon =
    lsp.status === 'initialized'
      ? '✓'
      : lsp.status === 'initializing'
        ? '◐'
        : lsp.status === 'failed'
          ? '✗'
          : '○';
  const color =
    lsp.status === 'initialized'
      ? colors.success
      : lsp.status === 'initializing'
        ? colors.warning
        : lsp.status === 'failed'
          ? colors.error
          : colors.secondary;
  const duration =
    lsp.initDurationMs != null
      ? ` (${formatDuration(lsp.initDurationMs)})`
      : '';
  return [
    { text: `${icon} ${lsp.name}`, color },
    { text: `(${lsp.languages.join(', ')})`, color: colors.secondary },
    { text: `${lsp.status}${duration}`, color },
  ];
}

function CodeLogsView({
  data,
  onClose,
}: {
  data: CodePanelData;
  onClose: () => void;
}) {
  const { getColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const secondary = getColor('secondary');
  const success = getColor('success');
  const warning = getColor('warning');
  const error = getColor('error');

  const entries = data.entries ?? [];
  const maxVisible = Math.max(termHeight - 9, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');

  const levelColor = (level: string) => {
    switch (level) {
      case 'ERROR':
        return error;
      case 'WARN':
        return warning;
      case 'INFO':
        return success;
      default:
        return secondary;
    }
  };

  const q = search.toLowerCase();
  const filtered = search
    ? entries.filter(
        (e) =>
          e.message.toLowerCase().includes(q) ||
          e.level.toLowerCase().includes(q)
      )
    : entries;

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
  }, []);

  return (
    <Panel
      title={`/code logs · ${data.level ?? 'ERROR'} · ${filtered.length} entries`}
      onClose={onClose}
      searchable={entries.length > 0}
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
      {entries.length === 0 ? (
        <Text>
          {secondary(`No logs at ${data.level ?? 'ERROR'} level or above`)}
        </Text>
      ) : filtered.length === 0 && search ? (
        <Text>{secondary('No matches')}</Text>
      ) : (
        visible.map((entry, i) => (
          <Text key={scrollOffset + i}>
            {secondary(entry.timestamp)}{' '}
            {levelColor(entry.level)(entry.level.padEnd(5))}{' '}
            {primary(entry.message)}
          </Text>
        ))
      )}
    </Panel>
  );
}
