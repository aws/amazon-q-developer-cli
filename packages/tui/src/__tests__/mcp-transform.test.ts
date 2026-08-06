import { describe, it, expect } from 'bun:test';
import {
  mcpServerNameFromTitle,
  normalizeToolCallTitle,
  stripMcpTitlePrefix,
  toolTelemetryStartFromEvent,
  unwrapKasMcpOutput,
} from '../acp-client';
import { AgentEventType } from '../types/agent-events';

describe('stripMcpTitlePrefix', () => {
  it('strips @serverName/ prefix from MCP tool titles', () => {
    expect(stripMcpTitlePrefix('@test-mock/echo')).toBe('echo');
    expect(stripMcpTitlePrefix('@my-server/get_weather')).toBe('get_weather');
  });

  it('preserves titles without @server/ prefix', () => {
    expect(stripMcpTitlePrefix('Running: echo hello')).toBe(
      'Running: echo hello'
    );
    expect(stripMcpTitlePrefix('Read File')).toBe('Read File');
    expect(stripMcpTitlePrefix('shell')).toBe('shell');
  });

  it('returns undefined for undefined input', () => {
    expect(stripMcpTitlePrefix(undefined)).toBeUndefined();
  });

  it('handles edge cases', () => {
    expect(stripMcpTitlePrefix('@server/tool/with/slashes')).toBe(
      'tool/with/slashes'
    );
    expect(stripMcpTitlePrefix('@/empty-server')).toBe('@/empty-server'); // no match — empty server name
    expect(stripMcpTitlePrefix('')).toBe('');
  });
});

describe('mcpServerNameFromTitle', () => {
  it('extracts the server name from @server/tool titles', () => {
    expect(mcpServerNameFromTitle('@test-mock/echo')).toBe('test-mock');
    expect(mcpServerNameFromTitle('@my-server/get_weather')).toBe('my-server');
  });

  it('returns undefined for non-MCP titles', () => {
    expect(mcpServerNameFromTitle('Read File')).toBeUndefined();
    expect(mcpServerNameFromTitle('shell')).toBeUndefined();
    expect(mcpServerNameFromTitle('@/empty-server')).toBeUndefined();
    expect(mcpServerNameFromTitle('')).toBeUndefined();
    expect(mcpServerNameFromTitle(undefined)).toBeUndefined();
  });
});

describe('toolTelemetryStartFromEvent', () => {
  it('records pipeline delegation as the canonical built-in tool', () => {
    expect(
      toolTelemetryStartFromEvent({
        type: AgentEventType.ToolCall,
        id: 'delegate-1',
        name: 'reviewer',
        args: {},
        meta: {
          kiro: { pipeline: { groupId: 'group-1', stages: [] } },
        },
      })
    ).toEqual({
      name: 'reviewer',
      toolOrigin: 'builtin',
      builtinToolName: 'use_subagent',
    });
  });
});

describe('normalizeToolCallTitle', () => {
  it('normalizes KAS MCP titles directly', () => {
    expect(normalizeToolCallTitle('@weather/get_forecast')).toEqual({
      name: 'get_forecast',
      origin: 'mcp',
      originalTitle: '@weather/get_forecast',
    });
  });

  it('uses V2 canonical metadata instead of its descriptive title', () => {
    expect(
      normalizeToolCallTitle(
        'Running: @weather/get_forecast',
        '@weather/get_forecast'
      )
    ).toEqual({
      name: 'get_forecast',
      origin: 'mcp',
      originalTitle: 'Running: @weather/get_forecast',
    });
    expect(normalizeToolCallTitle('Creating file.ts', 'fs_write')).toEqual({
      name: 'fs_write',
      origin: 'builtin',
      originalTitle: 'Creating file.ts',
    });
  });

  it('retains MCP provenance when canonical metadata is unqualified', () => {
    expect(
      normalizeToolCallTitle('@weather/get_forecast', 'get_forecast')
    ).toEqual({
      name: 'get_forecast',
      origin: 'mcp',
      originalTitle: '@weather/get_forecast',
    });
    expect(
      normalizeToolCallTitle('Running: @weather/get_forecast', 'get_forecast')
    ).toEqual({
      name: 'get_forecast',
      origin: 'mcp',
      originalTitle: 'Running: @weather/get_forecast',
    });
  });

  it('retains explicit MCP provenance when metadata collides with a built-in', () => {
    expect(
      normalizeToolCallTitle('@collision-server/fs_write', 'fs_write')
    ).toEqual({
      name: 'fs_write',
      origin: 'mcp',
      originalTitle: '@collision-server/fs_write',
    });
    expect(
      normalizeToolCallTitle('mcp__collision-server__fs_write', 'fs_write')
    ).toEqual({
      name: 'fs_write',
      origin: 'mcp',
      originalTitle: 'mcp__collision-server__fs_write',
    });
    expect(
      normalizeToolCallTitle('Running: @collision-server/fs_write', 'fs_write')
    ).toEqual({
      name: 'fs_write',
      origin: 'mcp',
      originalTitle: 'Running: @collision-server/fs_write',
    });
    expect(
      normalizeToolCallTitle('Running: @server/execute_bash', 'execute_bash')
    ).toEqual({
      name: 'execute_bash',
      origin: 'mcp',
      originalTitle: 'Running: @server/execute_bash',
    });
  });

  it('does not infer MCP provenance from scoped packages in built-in commands', () => {
    expect(
      normalizeToolCallTitle(
        'Running: npm install @scope/package',
        'execute_bash'
      )
    ).toEqual({
      name: 'execute_bash',
      origin: 'builtin',
      originalTitle: 'Running: npm install @scope/package',
    });
    expect(
      normalizeToolCallTitle('Running: @scope/package', 'execute_bash')
    ).toEqual({
      name: 'execute_bash',
      origin: 'builtin',
      originalTitle: 'Running: @scope/package',
    });
  });
});

describe('unwrapKasMcpOutput', () => {
  it('unwraps KAS MCP envelope to V2 content format', () => {
    const kasOutput = { response: '{"echoed": "hello"}', imageBase64Urls: [] };
    expect(unwrapKasMcpOutput(kasOutput)).toEqual({
      content: [{ type: 'text', text: '{"echoed": "hello"}' }],
    });
  });

  it('passes through V2 output format unchanged', () => {
    const v2Output = { content: [{ type: 'text', text: 'hello' }] };
    expect(unwrapKasMcpOutput(v2Output)).toEqual(v2Output);
  });

  it('passes through string output unchanged', () => {
    expect(unwrapKasMcpOutput('plain text')).toBe('plain text');
  });

  it('passes through null/undefined unchanged', () => {
    expect(unwrapKasMcpOutput(null)).toBeNull();
    expect(unwrapKasMcpOutput(undefined)).toBeUndefined();
  });

  it('passes through other object shapes unchanged', () => {
    const other = { exitCode: 0, output: 'hello' };
    expect(unwrapKasMcpOutput(other)).toEqual(other);
  });
});
