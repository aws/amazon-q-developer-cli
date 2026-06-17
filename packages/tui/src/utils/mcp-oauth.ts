import type { AgentEngine } from '../agent-engine.js';

export interface McpOAuthActionDeps {
  /** Which agent backend is active. */
  agentEngine: AgentEngine;
  /** Name of the MCP server requiring OAuth. */
  serverName: string;
  /** The OAuth authorization URL emitted by the agent, if known. */
  url: string | null | undefined;
  /** Reset the MCP server and (re)start its OAuth flow (KAS only). */
  resetMcpServer: (serverName: string, startOAuth: boolean) => Promise<void>;
  /** Copy text to the system clipboard. Returns false if no tool was found. */
  copyToClipboard: (text: string) => boolean;
  /** Surface a transient status message to the user. */
  showAlert: (
    message: string,
    status: 'info' | 'error',
    autoHideMs: number
  ) => void;
}

/**
 * Trigger MCP OAuth for a pending server. Behaviour differs by agent engine:
 *
 * - **KAS mode**: the initial connection used a placeholder redirect URI, so
 *   the cached URL is stale. Reset the server with `startOAuth=true` to spin up
 *   a real local redirect server and open the browser.
 * - **V2 (Rust agent) mode**: the agent already started a local redirect server
 *   on a real port and is blocked awaiting the callback, so the URL is valid
 *   as-is. The Rust agent does not implement `_kiro/mcp/resetServer`, so copy
 *   the URL for the user to open in a browser and complete authorization.
 */
export function startMcpOAuth(deps: McpOAuthActionDeps): void {
  const {
    agentEngine,
    serverName,
    url,
    resetMcpServer,
    copyToClipboard,
    showAlert,
  } = deps;

  if (!serverName) return;

  if (agentEngine === 'kas') {
    showAlert(`Authenticating MCP server "${serverName}"...`, 'info', 3000);
    resetMcpServer(serverName, true).catch(() => {
      showAlert(`Failed to start OAuth for "${serverName}"`, 'error', 5000);
    });
    return;
  }

  if (url && copyToClipboard(url)) {
    showAlert('OAuth URL copied to clipboard', 'info', 3000);
  } else {
    showAlert(
      'Failed to copy OAuth URL — no clipboard tool found',
      'error',
      5000
    );
  }
}
