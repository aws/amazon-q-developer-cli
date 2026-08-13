import React, { useState, useCallback, useMemo } from 'react';
import { useInput, Box } from '../../renderer.js';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Table, type Row } from './table/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import { useGlyphs, useAllowIcons } from '../../hooks/useGlyphs.js';
import { fuzzyScore } from '../../utils/fuzzyScore.js';
import {
  useAppStore,
  type McpServerInfo,
  type InitError,
  type CloudSnapshotReadiness,
} from '../../stores/app-store.js';
import { visibleWidth } from '../../utils/text-width.js';
import { copyToSystemClipboard } from '../../commands/effects.js';
import { features, Feature } from '../../features.js';
import {
  cloudPanelNotice,
  cloudPanelEmptyMessage,
  cloudNoticeLineCount,
} from './cloud-panel-notice.js';

interface McpPanelProps {
  servers: McpServerInfo[];
  registryServers?: McpServerInfo[];
  initErrors?: InitError[];
  pendingOAuthUrls?: Map<string, string>;
  mode: string;
  cloudSessionActive?: boolean;
  /** Sandbox snapshot readiness for this surface (cloud sessions only). */
  cloudSnapshotReadiness?: CloudSnapshotReadiness;
  onClose: () => void;
  onAction?: (serverNames: string[]) => Promise<void>;
  onAuthenticate?: (serverName: string) => void;
  /** Force OAuth (re-)authentication for the given remote server. */
  onForceAuth?: (serverName: string) => void;
  /** Abort a pending/forced authentication and reload the server normally. */
  onAbortAuth?: (serverName: string) => void;
  /** Remove persisted OAuth credentials for the given remote server. */
  onRemoveCredentials?: (serverName: string) => void;
}

const GAP = 2;

export const McpPanel: React.FC<McpPanelProps> = ({
  servers,
  registryServers = [],
  initErrors = [],
  pendingOAuthUrls = new Map(),
  mode,
  cloudSessionActive = false,
  cloudSnapshotReadiness,
  onClose,
  onAction,
  onAuthenticate,
  onForceAuth,
  onAbortAuth,
  onRemoveCredentials,
}) => {
  const { getColor } = useTheme();
  // Set by /config before this panel opens; footer hint only ('to go back'
  // vs 'to close') — back-navigation happens in the close handler (same
  // pattern as KeybindingsPanel's fromSettings).
  const fromConfig = useAppStore((state) => state.configReturnOnEscape);
  const { width: termWidth, height: termHeight } = useTerminalSize();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const info = getColor('info');
  const success = getColor('success');
  const warning = getColor('warning');
  const error = getColor('error');

  const statusLabels: Record<McpServerInfo['status'], string> = useMemo(
    () => ({
      running: `${!allowIcons ? '' : glyphs.dotFilled} running`,
      loading: `${!allowIcons ? '' : glyphs.dotLoading} loading`,
      failed: `${!allowIcons ? '' : glyphs.cross} failed`,
      disabled: `${!allowIcons ? '' : glyphs.dotEmpty} disabled`,
      'auth-required': `${!allowIcons ? '' : glyphs.warning} auth-required`,
    }),
    [glyphs, allowIcons]
  );

  const isRegistryView =
    servers.length > 0 && servers[0]?.version !== undefined;
  const isInteractive =
    (mode === 'add' || mode === 'remove') && isRegistryView && !!onAction;

  // Source column (Figma frames 24/27): only inside the cloud_config rollout
  // AND when at least one server carries a source — off-cohort output stays
  // byte-identical (same pattern as ToolsPanel's optional Status column).
  const showSource =
    features.isEnabled(Feature.CloudConfig) && servers.some((s) => s.source);

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

  const cloudNotice = cloudPanelNotice(
    'mcp',
    cloudSessionActive,
    cloudSnapshotReadiness
  );
  const awaitingSandbox =
    cloudSessionActive && cloudSnapshotReadiness === 'awaiting-sandbox';
  const noticeLines = cloudNoticeLineCount(
    cloudNotice,
    termWidth - 2,
    !awaitingSandbox && servers.length > 0
  );
  const maxVisible = Math.max(termHeight - 9 - noticeLines, 5);
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
        ...(showSource ? [{ label: 'Source', width: 8 + GAP }] : []),
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
                  text: isSelected ? `[${glyphs.checkmark}]` : '[ ]',
                  color: isSelected ? success : dim,
                },
              ]
            : [];

          let statusText: string;
          let statusClr: (s: string) => string;
          if (isPending) {
            statusText =
              mode === 'add'
                ? `${!allowIcons ? '' : glyphs.dotLoading} adding...`
                : `${!allowIcons ? '' : glyphs.dotLoading} removing...`;
            statusClr = warning;
          } else if (server.enabled) {
            statusText = `${!allowIcons ? '' : glyphs.checkmark} enabled`;
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
          detail = `${server.status} ${glyphs.smallDot} Enter to authenticate`;
        } else if (server.status === 'failed' && reason) {
          detail = reason;
        } else {
          detail = `${server.toolCount} tool${server.toolCount === 1 ? '' : 's'}`;
        }
        const detailColor = server.status === 'failed' && reason ? error : dim;
        return [
          { text: server.name, color: primary },
          ...(showSource ? [{ text: server.source ?? '', color: dim }] : []),
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
      showSource,
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
      statusLabels,
    ]
  );

  // Row navigation (^J / ^K) works in both the interactive registry view and the
  // non-interactive status view.
  useInput((input: string, key: { ctrl: boolean }) => {
    if (pending.size > 0) return;
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
  });

  // Interactive registry view (/mcp add | remove): Tab selects, Enter submits.
  useInput((_input: string, key: { tab: boolean; return: boolean }) => {
    if (!isInteractive || pending.size > 0) return;
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
  });

  // Status view (/mcp): act on the highlighted server.
  //   Enter → authenticate a server with a pending OAuth request
  //   ^A    → force OAuth (re-)authentication
  //   ^X    → abort a pending/forced authentication
  //   ^R    → remove persisted OAuth credentials
  useInput((input: string, key: { ctrl: boolean; return: boolean }) => {
    if (isInteractive) return;
    const server = filtered[cursorIndex];
    if (!server) return;
    if (key.return) {
      if (pendingOAuthUrls.has(server.name)) {
        if (onAuthenticate) {
          onAuthenticate(server.name);
        } else {
          const url = pendingOAuthUrls.get(server.name);
          if (url) void copyToSystemClipboard(url);
        }
      }
      return;
    }
    if (key.ctrl && input === 'a') {
      onForceAuth?.(server.name);
      return;
    }
    if (key.ctrl && input === 'x') {
      onAbortAuth?.(server.name);
      return;
    }
    if (key.ctrl && input === 'r') {
      onRemoveCredentials?.(server.name);
      return;
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
        const statusText = server.enabled
          ? `${!allowIcons ? '' : glyphs.checkmark} enabled`
          : '  disabled';
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
  // Routed from /config, the panel titles itself as the MCP category page
  // ('/config — MCP', frames 24/27) so it reads like the other /config
  // pages; the view itself is identical to /mcp. Direct /mcp is unchanged,
  // including its 'list' mode word (redundant after a '/config — MCP' label).
  const commandLabel = fromConfig ? '/config — MCP' : '/mcp';
  const listLabel = fromConfig ? commandLabel : '/mcp list';
  const title = isRegistryView
    ? `${commandLabel} ${modeLabel} ${glyphs.smallDot} ${servers.length} server${servers.length === 1 ? '' : 's'}${selCount > 0 ? ` ${glyphs.smallDot} ${selCount} selected` : ''}`
    : isListMode
      ? `${listLabel} ${glyphs.smallDot} ${servers.length} configured${hasRegistry ? `, ${registryServers.length} registry` : ''}`
      : `${commandLabel} ${glyphs.smallDot} ${servers.length} server${servers.length === 1 ? '' : 's'}`;

  const governanceDisabled = initErrors.find(
    (e): e is Extract<InitError, { type: 'mcp_governance_disabled' }> =>
      e.type === 'mcp_governance_disabled'
  );

  const emptyMessage = governanceDisabled
    ? governanceDisabled.apiFailure
      ? `${!allowIcons ? '' : glyphs.warning} Failed to retrieve MCP settings — MCP disabled`
      : `${!allowIcons ? '' : glyphs.warning} MCP has been disabled by your administrator`
    : cloudSessionActive && !isRegistryView
      ? cloudPanelEmptyMessage('mcp')
      : isRegistryView
        ? 'No servers in MCP registry'
        : 'No MCP servers configured';

  const isStatusView = !isRegistryView && !isListMode;

  // Frame-24/27 footer: conflict-precedence + edit paths. cloud_config
  // rollout only, so off-cohort /mcp output stays byte-identical. The
  // conflict line appears only when local and cloud servers actually
  // coexist (the fact it explains); cloud-session views get the cloud-edit
  // line alone (frame 27).
  const showSourceFooter =
    features.isEnabled(Feature.CloudConfig) &&
    !isRegistryView &&
    servers.length > 0;
  const hasCloudSourced = servers.some((s) => s.source === 'cloud');
  const hasLocalSourced = servers.some((s) => s.source === 'local');
  const sourceFooterLines = showSourceFooter
    ? [
        ...(hasCloudSourced && hasLocalSourced
          ? ['In case of conflict, local will override cloud configurations']
          : []),
        ...(!cloudSessionActive
          ? [
              'To edit local configs: Just ask Kiro, or open ~/.kiro/settings/mcp.json',
            ]
          : []),
        ...(hasCloudSourced || cloudSessionActive
          ? ['To edit cloud configs: https://app.kiro.dev/settings']
          : []),
      ]
    : [];
  const statusActionHints = [
    onForceAuth && { key: '^A', label: 'auth' },
    onAbortAuth && { key: '^X', label: 'abort' },
    onRemoveCredentials && { key: '^R', label: 'remove creds' },
  ].filter((hint): hint is { key: string; label: string } => !!hint);

  const footerExtra = isInteractive ? (
    <Text>
      {primary('^J/K')} {dim('navigate')} {dim(glyphs.smallDot)}{' '}
      {primary('Tab')} {dim('select')} {dim(glyphs.smallDot)} {primary('Enter')}{' '}
      {dim(mode)}
    </Text>
  ) : isStatusView && servers.length > 0 ? (
    <Text>
      {primary('^J/K')} {dim('navigate')}
      {statusActionHints.map(({ key, label }) => (
        <React.Fragment key={key}>
          {' '}
          {dim(glyphs.smallDot)} {primary(key)} {dim(label)}
        </React.Fragment>
      ))}
    </Text>
  ) : undefined;

  return (
    <Panel
      title={title}
      onClose={onClose}
      closeHintLabel={fromConfig ? 'to go back' : 'to close'}
      searchable={!governanceDisabled}
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
      <Box flexDirection="column">
        {cloudNotice && (
          <Box
            marginBottom={
              !awaitingSandbox && (hasConfigured || hasRegistry) ? 1 : 0
            }
          >
            <Text>{info(cloudNotice)}</Text>
          </Box>
        )}
        {awaitingSandbox ? null : !hasConfigured && !hasRegistry ? (
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
              isInteractive || isStatusView
                ? cursorIndex - scrollOffset
                : undefined
            }
          />
        )}
      </Box>
      {sourceFooterLines.length > 0 && (
        <Box flexDirection="column" marginTop={1}>
          {sourceFooterLines.map((line, i) => (
            <Text key={i}>{dim(line)}</Text>
          ))}
        </Box>
      )}
    </Panel>
  );
};
