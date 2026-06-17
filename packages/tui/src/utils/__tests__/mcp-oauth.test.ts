import { describe, expect, it, mock } from 'bun:test';
import { startMcpOAuth } from '../mcp-oauth';

function makeDeps(
  overrides: Partial<Parameters<typeof startMcpOAuth>[0]> = {}
) {
  const resetMcpServer = mock(() => Promise.resolve());
  const copyToClipboard = mock(() => true);
  const showAlert = mock(() => {});
  return {
    deps: {
      agentEngine: 'v2' as const,
      serverName: 'oauth-srv',
      url: 'https://example.com/auth',
      resetMcpServer,
      copyToClipboard,
      showAlert,
      ...overrides,
    },
    resetMcpServer,
    copyToClipboard,
    showAlert,
  };
}

describe('startMcpOAuth', () => {
  it('V2 mode: copies the URL to the clipboard and does not reset the server', () => {
    const { deps, resetMcpServer, copyToClipboard, showAlert } = makeDeps({
      agentEngine: 'v2',
    });

    startMcpOAuth(deps);

    expect(copyToClipboard).toHaveBeenCalledWith('https://example.com/auth');
    expect(resetMcpServer).not.toHaveBeenCalled();
    expect(showAlert).toHaveBeenCalledWith(
      'OAuth URL copied to clipboard',
      'info',
      3000
    );
  });

  it('V2 mode: reports an error when the clipboard copy fails', () => {
    const { deps, showAlert } = makeDeps({
      agentEngine: 'v2',
      copyToClipboard: mock(() => false),
    });

    startMcpOAuth(deps);

    expect(showAlert).toHaveBeenCalledWith(
      'Failed to copy OAuth URL — no clipboard tool found',
      'error',
      5000
    );
  });

  it('V2 mode: reports an error when no URL is available', () => {
    const { deps, copyToClipboard, showAlert } = makeDeps({
      agentEngine: 'v2',
      url: null,
    });

    startMcpOAuth(deps);

    expect(copyToClipboard).not.toHaveBeenCalled();
    expect(showAlert).toHaveBeenCalledWith(
      'Failed to copy OAuth URL — no clipboard tool found',
      'error',
      5000
    );
  });

  it('KAS mode: resets the server to start OAuth and does not copy the URL', () => {
    const { deps, resetMcpServer, copyToClipboard, showAlert } = makeDeps({
      agentEngine: 'kas',
    });

    startMcpOAuth(deps);

    expect(resetMcpServer).toHaveBeenCalledWith('oauth-srv', true);
    expect(copyToClipboard).not.toHaveBeenCalled();
    expect(showAlert).toHaveBeenCalledWith(
      'Authenticating MCP server "oauth-srv"...',
      'info',
      3000
    );
  });

  it('does nothing when serverName is empty', () => {
    const { deps, resetMcpServer, copyToClipboard, showAlert } = makeDeps({
      serverName: '',
    });

    startMcpOAuth(deps);

    expect(resetMcpServer).not.toHaveBeenCalled();
    expect(copyToClipboard).not.toHaveBeenCalled();
    expect(showAlert).not.toHaveBeenCalled();
  });
});
