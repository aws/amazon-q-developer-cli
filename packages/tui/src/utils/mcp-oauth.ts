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

/** Shown when the OAuth URL is (or will be) on the clipboard. */
const URL_COPIED_MESSAGE = 'OAuth URL copied to clipboard';

/**
 * Trigger MCP OAuth for a pending server. Behaviour differs by agent engine,
 * but the user-facing outcome is the same: the OAuth URL ends up on the
 * clipboard for the user to open in a browser.
 *
 * - **KAS mode**: the initial connection used a placeholder redirect URI, so
 *   the cached URL is stale. Reset the server with `startOAuth=true`; KAS then
 *   regenerates a valid URL and hands it back via `_kiro/openExternalUrl`,
 *   which the CLI copies to the clipboard (see `capabilities/copy-url-to-clipboard.ts`).
 * - **V2 (Rust agent) mode**: the agent already started a local redirect server
 *   on a real port and is blocked awaiting the callback, so the URL is valid
 *   as-is. The Rust agent does not implement `_kiro/mcp/resetServer`, so copy
 *   the URL here directly.
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
    showAlert(URL_COPIED_MESSAGE, 'info', 3000);
    resetMcpServer(serverName, true).catch(() => {
      showAlert(`Failed to start OAuth for "${serverName}"`, 'error', 5000);
    });
    return;
  }

  if (url && copyToClipboard(url)) {
    showAlert(URL_COPIED_MESSAGE, 'info', 3000);
  } else {
    showAlert(
      'Failed to copy OAuth URL — no clipboard tool found',
      'error',
      5000
    );
  }
}
