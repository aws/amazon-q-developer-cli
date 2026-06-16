import { describe, it, expect } from 'bun:test';
import {
  mapSessionToolTagToToolInfo,
  parseToolsDidChange,
  type SessionToolTag,
} from '../kas-tools';

describe('mapSessionToolTagToToolInfo', () => {
  it('maps a builtin tag to ToolInfo without status', () => {
    const tag: SessionToolTag = {
      source: 'builtin',
      tag: 'read',
      description: 'read-file, diagnostics, search tools',
    };
    expect(mapSessionToolTagToToolInfo(tag)).toEqual({
      name: 'read',
      source: 'builtin',
      description: 'read-file, diagnostics, search tools',
    });
  });

  it('maps an mcp per-tool tag', () => {
    const tag: SessionToolTag = {
      source: 'mcp',
      tag: '@git/status',
      description: 'Show working tree status',
    };
    expect(mapSessionToolTagToToolInfo(tag)).toEqual({
      name: '@git/status',
      source: 'mcp',
      description: 'Show working tree status',
    });
  });

  it('never sets a status field (KAS has no per-tool status)', () => {
    const info = mapSessionToolTagToToolInfo({
      source: 'builtin',
      tag: 'write',
      description: 'write-file tools',
    });
    expect(info.status).toBeUndefined();
  });
});

describe('parseToolsDidChange', () => {
  it('parses a well-formed payload', () => {
    const result = parseToolsDidChange({
      sessionId: 's1',
      tags: [
        { source: 'builtin', tag: 'read', description: 'read tools' },
        { source: 'mcp', tag: '@git/status', description: 'git status' },
      ],
    });
    expect(result).toEqual([
      { name: 'read', source: 'builtin', description: 'read tools' },
      { name: '@git/status', source: 'mcp', description: 'git status' },
    ]);
  });

  it('returns [] when tags is missing or not an array', () => {
    expect(parseToolsDidChange({})).toEqual([]);
    expect(parseToolsDidChange({ tags: 'nope' })).toEqual([]);
    expect(parseToolsDidChange({ tags: null })).toEqual([]);
  });

  it('skips malformed entries (missing tag or bad source)', () => {
    const result = parseToolsDidChange({
      tags: [
        { source: 'builtin', tag: 'read', description: 'ok' },
        { source: 'unknown', tag: 'x', description: 'bad source' },
        { source: 'mcp', description: 'missing tag' },
        null,
        42,
      ],
    });
    expect(result).toEqual([
      { name: 'read', source: 'builtin', description: 'ok' },
    ]);
  });

  it('falls back to the tag string when description is absent/blank', () => {
    const result = parseToolsDidChange({
      tags: [{ source: 'mcp', tag: '@git/log' }],
    });
    expect(result).toEqual([
      { name: '@git/log', source: 'mcp', description: '@git/log' },
    ]);
  });
});
