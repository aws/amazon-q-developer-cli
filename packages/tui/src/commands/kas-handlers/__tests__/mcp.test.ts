import { describe, expect, it, mock } from 'bun:test';
import { KasCommandName, type KasCommand } from '../../../kas-commands';
import type { McpServerInfo } from '../../../stores/app-store';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import { handleMcp } from '../mcp';

const mcpCmd: KasCommand = {
  name: KasCommandName.Mcp,
  description: 'Show MCP server status',
  meta: { inputType: 'panel' },
};

describe('/mcp KAS handler', () => {
  it('opens configured servers from the store-owned snapshot', async () => {
    const servers: McpServerInfo[] = [
      { name: 'github', status: 'running', toolCount: 2 },
    ];
    const executeCommand = mock(() =>
      Promise.resolve({ success: true, message: '' })
    );
    const ctx = createMockCommandContext({
      mcpServerCache: servers,
      kiro: { executeCommand },
    });

    await handleMcp(mcpCmd, '', ctx);

    expect(executeCommand).not.toHaveBeenCalled();
    expect(ctx._spies.setActiveCommand).toHaveBeenCalled();
    expect(ctx._spies.setShowMcpPanel).toHaveBeenCalledWith(
      true,
      servers,
      'list',
      undefined
    );
  });

  it('combines configured servers with the store-owned registry snapshot', async () => {
    const servers: McpServerInfo[] = [
      { name: 'github', status: 'running', toolCount: 2 },
    ];
    const registry: McpServerInfo[] = [
      { name: 'memory', status: 'disabled', toolCount: 0, enabled: false },
    ];
    const executeCommand = mock(() =>
      Promise.resolve({ success: true, message: '' })
    );
    const ctx = createMockCommandContext({
      mcpServerCache: servers,
      mcpRegistryCache: registry,
      kiro: { executeCommand },
    });

    await handleMcp(mcpCmd, 'list', ctx);

    expect(executeCommand).not.toHaveBeenCalled();
    expect(ctx._spies.setShowMcpPanel).toHaveBeenCalledWith(
      true,
      servers,
      'list',
      registry
    );
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      '1 configured, 1 registry servers',
      'success',
      5000
    );
  });

  it('preserves the configured-server message for other arguments', async () => {
    const ctx = createMockCommandContext({
      mcpServerCache: [{ name: 'github', status: 'running', toolCount: 2 }],
    });

    await handleMcp(mcpCmd, 'auth github', ctx);

    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      '1 configured server',
      'success',
      5000
    );
  });
});
