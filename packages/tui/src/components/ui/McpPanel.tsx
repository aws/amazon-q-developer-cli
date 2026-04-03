import React, { useMemo, useCallback } from 'react';
import { Text } from './text/Text';
import { Panel } from './panel/index.js';
import { Menu, type MenuItem } from './menu/index.js';
import { useTheme } from '../../hooks/useThemeContext';
import { useTerminalSize } from '../../hooks/useTerminalSize';
import type { McpServerInfo, InitError } from '../../stores/app-store.js';
import { copyToSystemClipboard } from '../../commands/effects.js';

interface McpPanelProps {
  servers: McpServerInfo[];
  initErrors?: InitError[];
  pendingOAuthUrls?: Map<string, string>;
  onClose: () => void;
}

const statusPrefix: Record<McpServerInfo['status'], string> = {
  running: '●',
  loading: '◌',
  failed: '✕',
  disabled: '○',
  'auth-required': '⚠',
};

export const McpPanel: React.FC<McpPanelProps> = ({
  servers,
  initErrors = [],
  pendingOAuthUrls = new Map(),
  onClose,
}) => {
  const { getColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const dim = getColor('secondary');

  const failureReasons = useMemo(() => {
    const map = new Map<string, string>();
    for (const e of initErrors) {
      if (e.type === 'mcp_failure') {
        map.set(e.serverName, e.error);
      }
    }
    return map;
  }, [initErrors]);

  const items: MenuItem[] = useMemo(
    () =>
      servers.map((server) => {
        const prefix = statusPrefix[server.status];
        const reason = failureReasons.get(server.name);
        const hasOAuth = pendingOAuthUrls.has(server.name);
        let desc: string;
        if (hasOAuth) {
          desc = `${server.status} · Enter to copy OAuth URL`;
        } else if (server.status === 'failed' && reason) {
          desc = reason;
        } else {
          desc = `${server.status} · ${server.toolCount} tool${server.toolCount === 1 ? '' : 's'}`;
        }
        return { label: `${prefix} ${server.name}`, description: desc };
      }),
    [servers, failureReasons, pendingOAuthUrls]
  );

  const handleSelect = useCallback(
    (item: MenuItem) => {
      // Extract server name from label (skip the status prefix + space)
      const name = item.label.slice(2);
      const url = pendingOAuthUrls.get(name);
      if (url) {
        copyToSystemClipboard(url);
      }
    },
    [pendingOAuthUrls]
  );

  return (
    <Panel
      title={`/mcp · ${servers.length} server${servers.length === 1 ? '' : 's'}`}
      onClose={onClose}
    >
      {servers.length === 0 ? (
        <Text>{dim('No MCP servers configured')}</Text>
      ) : (
        <Menu
          items={items}
          onSelect={handleSelect}
          visibleItems={Math.max(termHeight - 9, 5)}
          showSelectedIndicator={true}
          searchable={true}
          searchLabel="search"
          searchPlaceholder="type to filter"
        />
      )}
    </Panel>
  );
};
