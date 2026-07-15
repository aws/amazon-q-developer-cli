import { describe, it, expect } from 'bun:test';
import {
  mcpServerNameFromTitle,
  stripMcpTitlePrefix,
  unwrapKasMcpOutput,
} from '../acp-client';

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
