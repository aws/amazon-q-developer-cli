import { describe, expect, it, mock } from 'bun:test';
import { startMcpOAuth } from '../mcp-oauth';

function makeDeps(
  overrides: Partial<Parameters<typeof startMcpOAuth>[0]> = {}
) {
  const resetMcpServer = mock(() => Promise.resolve());
  const copyToClipboard = mock(() => true);
  const showAlert = mock(() => {});
  const addSystemMessage = mock(() => {});
  return {
    deps: {
      agentEngine: 'v2' as const,
      serverName: 'oauth-srv',
      url: 'https://example.com/auth',
      resetMcpServer,
      copyToClipboard,
      showAlert,
      addSystemMessage,
      ...overrides,
    },
    resetMcpServer,
    copyToClipboard,
    showAlert,
    addSystemMessage,
  };
}

describe('startMcpOAuth', () => {
  it('V2 mode: copies the URL without adding a fallback row', () => {
    const {
      deps,
      resetMcpServer,
      copyToClipboard,
      showAlert,
      addSystemMessage,
    } = makeDeps({
      agentEngine: 'v2',
    });

    startMcpOAuth(deps);

    expect(copyToClipboard).toHaveBeenCalledWith('https://example.com/auth');
    expect(resetMcpServer).not.toHaveBeenCalled();
    expect(addSystemMessage).not.toHaveBeenCalled();
    expect(showAlert).toHaveBeenCalledWith(
      'OAuth URL copied to clipboard',
      'info',
      3000
    );
  });

  it('V2 mode: adds one persistent sensitive-URL fallback when copying fails', () => {
    const { deps, showAlert, addSystemMessage } = makeDeps({
      agentEngine: 'v2',
      copyToClipboard: mock(() => false),
    });

    startMcpOAuth(deps);

    expect(addSystemMessage).toHaveBeenCalledTimes(1);
    expect(addSystemMessage).toHaveBeenCalledWith(
      'Clipboard copy failed. Open this session-specific OAuth URL manually; do not share it:\nhttps://example.com/auth',
      false
    );
    expect(showAlert).toHaveBeenCalledWith(
      'Failed to copy OAuth URL — no clipboard tool found',
      'error',
      5000
    );
    expect(showAlert).not.toHaveBeenCalledWith(
      expect.stringContaining('https://example.com/auth'),
      expect.anything(),
      expect.anything()
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
      'OAuth URL copied to clipboard',
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
