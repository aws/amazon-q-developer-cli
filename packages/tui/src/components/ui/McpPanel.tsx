import React, { useState, useCallback, useMemo } from 'react';
import { useInput, Box } from '../../renderer.js';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import type { McpServerInfo, InitError } from '../../stores/app-store.js';
import { visibleWidth } from '../../utils/text-width.js';
import { copyToSystemClipboard } from '../../commands/effects.js';

interface McpPanelProps {
  servers: McpServerInfo[];
  registryServers?: McpServerInfo[];
  initErrors?: InitError[];
  pendingOAuthUrls?: Map<string, string>;
  mode: string;
  onClose: () => void;
  onAction?: (serverNames: string[]) => Promise<void>;
}

const statusLabels: Record<McpServerInfo['status'], string> = {
  running: '● running',
  loading: '◌ loading',
  failed: '✕ failed',
  disabled: '○ disabled',
  'auth-required': '⚠ auth-required',
};

const GAP = 2;

export const McpPanel: React.FC<McpPanelProps> = ({
  servers,
  registryServers = [],
  initErrors = [],
  pendingOAuthUrls = new Map(),
  mode,
  onClose,
  onAction,
}) => {
  const { getColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const success = getColor('success');
  const warning = getColor('warning');
  const error = getColor('error');

  const isRegistryView =
    servers.length > 0 && servers[0]?.version !== undefined;
  const isInteractive = (mode === 'add' || mode === 'remove') && isRegistryView;

  // Build a lookup of MCP failure reasons from initErrors
  const failureReasons = useMemo(() => {
    const map = new Map<string, string>();
    for (const e of initErrors) {
      if (e.type === 'mcp_failure') {
        map.set(e.serverName, e.error);
      }
    }
    return map;
  }, [initErrors]);

  const maxVisible = Math.max(termHeight - 9, 5);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [search, setSearch] = useState('');
  const [cursorIndex, setCursorIndex] = useState(0);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<Set<string>>(new Set());

  const q = search.toLowerCase();
  const filtered = search
    ? servers
        .map((s) => ({
          s,
          score: Math.max(
            fuzzyScore(q, s.name.toLowerCase()),
            fuzzyScore(q, (s.description ?? '').toLowerCase())
          ),
        }))
        .filter(({ score }) => score > 0)
        .sort((a, b) => b.score - a.score)
        .map(({ s }) => s)
    : servers;

  const canScrollDown = scrollOffset + maxVisible < filtered.length;
  const visible = filtered.slice(scrollOffset, scrollOffset + maxVisible);

  const maxNameLen = servers.reduce(
    (max, s) => Math.max(max, visibleWidth(s.name)),
    0
  );
  const nameCol = Math.max(maxNameLen, 12) + GAP;

  const statusColor = (status: McpServerInfo['status']) => {
    switch (status) {
      case 'running':
        return success;
      case 'loading':
        return warning;
      case 'failed':
        return error;
      case 'disabled':
        return dim;
      case 'auth-required':
        return warning;
    }
  };

  const columns = isRegistryView
    ? [
        ...(isInteractive ? [{ label: '', width: 4 }] : []),
        { label: 'Name', width: nameCol },
        { label: 'Status', width: 14 + GAP },
        { label: 'Version', width: 12 + GAP },
        { label: 'Description' },
      ]
    : [
        { label: 'Name', width: nameCol },
        { label: 'Status', width: 14 + GAP },
        { label: 'Details' },
      ];

  const rows: Row[] = useMemo(
    () =>
      visible.map((server) => {
        if (isRegistryView) {
          const isSelected = selected.has(server.name);
          const isPending = pending.has(server.name);
          const checkbox = isInteractive
            ? [
                {
                  text: isSelected ? '[✓]' : '[ ]',
                  color: isSelected ? success : dim,
                },
              ]
            : [];

          let statusText: string;
          let statusClr: (s: string) => string;
          if (isPending) {
            statusText = mode === 'add' ? '◌ adding...' : '◌ removing...';
            statusClr = warning;
          } else if (server.enabled) {
            statusText = '✓ enabled';
            statusClr = success;
          } else {
            statusText = '  disabled';
            statusClr = dim;
          }

          return [
            ...checkbox,
            { text: server.name, color: primary },
            { text: statusText, color: statusClr },
            { text: server.version ?? '', color: dim },
            { text: server.description ?? '', color: dim },
          ];
        }
        // Status view: show failure reasons from initErrors + OAuth info
        const reason = failureReasons.get(server.name);
        const hasOAuth = pendingOAuthUrls.has(server.name);
        let detail: string;
        if (hasOAuth) {
          detail = `${server.status} · Enter to copy OAuth URL`;
        } else if (server.status === 'failed' && reason) {
          detail = reason;
        } else {
          detail = `${server.toolCount} tool${server.toolCount === 1 ? '' : 's'}`;
        }
        const detailColor = server.status === 'failed' && reason ? error : dim;
        return [
          { text: server.name, color: primary },
          {
            text: statusLabels[server.status] ?? server.status,
            color: statusColor(server.status),
          },
          { text: detail, color: detailColor },
        ];
      }),
    [
      visible,
      isRegistryView,
      isInteractive,
      selected,
      pending,
      mode,
      primary,
      dim,
      success,
      warning,
      error,
      failureReasons,
      pendingOAuthUrls,
    ]
  );

  useInput(
    (input: string, key: { ctrl: boolean; return: boolean; tab: boolean }) => {
      if (!isInteractive || pending.size > 0) return;
      if (key.ctrl && input === 'j') {
        setCursorIndex((prev) => {
          const next = Math.min(prev + 1, filtered.length - 1);
          if (next >= scrollOffset + maxVisible)
            setScrollOffset(next - maxVisible + 1);
          return next;
        });
        return;
      }
      if (key.ctrl && input === 'k') {
        setCursorIndex((prev) => {
          const next = Math.max(prev - 1, 0);
          if (next < scrollOffset) setScrollOffset(next);
          return next;
        });
        return;
      }
      if (key.tab) {
        const server = filtered[cursorIndex];
        if (server) {
          setSelected((prev) => {
            const next = new Set(prev);
            if (next.has(server.name)) next.delete(server.name);
            else next.add(server.name);
            return next;
          });
        }
        return;
      }
      if (key.return) {
        // In interactive mode, Enter submits selected servers
        if (selected.size > 0 && onAction) {
          const names = Array.from(selected);
          setPending(new Set(names));
          setSelected(new Set());
          onAction(names).finally(() => setPending(new Set()));
        }
        return;
      }
    }
  );

  // In status view (non-interactive), handle Enter to copy OAuth URL
  useInput((_input: string, key: { return: boolean }) => {
    if (!key.return || isInteractive) return;
    const server = filtered[cursorIndex];
    if (server) {
      const url = pendingOAuthUrls.get(server.name);
      if (url) {
        void copyToSystemClipboard(url);
      }
    }
  });

  const handleSearchChange = useCallback((s: string) => {
    setSearch(s);
    setScrollOffset(0);
    setCursorIndex(0);
  }, []);

  // Registry table for /mcp list mode
  const registryNameCol =
    registryServers.length > 0
      ? Math.max(
          registryServers.reduce(
            (max, s) => Math.max(max, visibleWidth(s.name)),
            0
          ),
          12
        ) + GAP
      : nameCol;

  const registryColumns = [
    { label: 'Name', width: registryNameCol },
    { label: 'Status', width: 14 + GAP },
    { label: 'Version', width: 12 + GAP },
    { label: 'Description' },
  ];

  const registryRows: Row[] = useMemo(
    () =>
      registryServers.map((server) => {
        const statusText = server.enabled ? '✓ enabled' : '  disabled';
        const statusClr = server.enabled ? success : dim;
        return [
          { text: server.name, color: primary },
          { text: statusText, color: statusClr },
          { text: server.version ?? '', color: dim },
          { text: server.description ?? '', color: dim },
        ];
      }),
    [registryServers, primary, dim, success]
  );

  const hasConfigured = servers.length > 0;
  const hasRegistry = registryServers.length > 0;
  const isListMode = mode === 'list';

  const modeLabel =
    mode === 'add' ? 'add' : mode === 'remove' ? 'remove' : 'list';
  const selCount = selected.size + pending.size;
  const title = isRegistryView
    ? `/mcp ${modeLabel} · ${servers.length} server${servers.length === 1 ? '' : 's'}${selCount > 0 ? ` · ${selCount} selected` : ''}`
    : isListMode
      ? `/mcp list · ${servers.length} configured${hasRegistry ? `, ${registryServers.length} registry` : ''}`
      : `/mcp · ${servers.length} server${servers.length === 1 ? '' : 's'}`;

  const emptyMessage = isRegistryView
    ? 'No servers in MCP registry'
    : 'No MCP servers configured';

  const footerExtra = isInteractive ? (
    <Text>
      {primary('^J/K')} {dim('navigate')} {dim('·')} {primary('Tab')}{' '}
      {dim('select')} {dim('·')} {primary('Enter')} {dim(mode)}
    </Text>
  ) : undefined;

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
      footerExtra={footerExtra}
    >
      {!hasConfigured && !hasRegistry ? (
        <Text>{dim(emptyMessage)}</Text>
      ) : isListMode && !isRegistryView ? (
        <Box flexDirection="column">
          {hasConfigured && (
            <Box flexDirection="column">
              <Text>{primary.bold('Configured Servers')}</Text>
              <Table columns={columns} rows={rows} />
            </Box>
          )}
          {hasRegistry && (
            <Box flexDirection="column" marginTop={hasConfigured ? 1 : 0}>
              <Text>{primary.bold('Registry Servers')}</Text>
              <Table columns={registryColumns} rows={registryRows} />
            </Box>
          )}
        </Box>
      ) : (
        <Table
          columns={columns}
          rows={rows}
          highlightedRow={
            isInteractive ? cursorIndex - scrollOffset : undefined
          }
        />
      )}
    </Panel>
  );
};
