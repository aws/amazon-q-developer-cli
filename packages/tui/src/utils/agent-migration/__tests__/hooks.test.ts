/**
 * Unit tests for `convertHooks` — CLI object-form `hooks` → KAS array-form.
 * Ports the behavioral cases from the Rust reference (`migration/hooks.rs`).
 */

import { describe, test, expect } from 'bun:test';

import { convertHooks } from '../hooks.js';
import type { MigrationWarning } from '../permissions.js';

function convert(hooks: unknown): { out: unknown; warnings: string[] } {
  const warnings: MigrationWarning[] = [];
  const out = convertHooks(hooks, warnings);
  return { out, warnings: warnings.map((w) => w.kind) };
}

describe('convertHooks', () => {
  test('object form becomes KAS array', () => {
    const { out, warnings } = convert({
      agentSpawn: [{ command: 'git status' }],
    });
    expect(out).toEqual([
      {
        name: 'agentSpawn-0',
        trigger: 'agentSpawn',
        action: { type: 'command', command: 'git status' },
        timeout: 10,
      },
    ]);
    expect(warnings).toEqual([]);
  });

  test('non-default fields and matcher carry over with unit conversion', () => {
    const { out } = convert({
      preToolUse: [
        {
          command: 'fmt',
          matcher: 'fs_write',
          timeout_ms: 5000,
          max_output_size: 2048,
          cache_ttl_seconds: 60,
        },
      ],
    });
    expect(out).toEqual([
      {
        name: 'preToolUse-0',
        trigger: 'preToolUse',
        matcher: 'fs_write',
        action: { type: 'command', command: 'fmt' },
        timeout: 5,
        maxOutputSize: 2048,
        cacheTtlSeconds: 60,
      },
    ]);
  });

  test('default-valued extras are omitted', () => {
    const { out } = convert({
      stop: [{ command: 'x', max_output_size: 10240, cache_ttl_seconds: 0 }],
    });
    const doc = (out as Record<string, unknown>[])[0]!;
    expect('maxOutputSize' in doc).toBe(false);
    expect('cacheTtlSeconds' in doc).toBe(false);
  });

  test('timeout always emitted even at a default', () => {
    const { out } = convert({ stop: [{ command: 'x', timeout_ms: 10000 }] });
    expect((out as Record<string, unknown>[])[0]!.timeout).toBe(10);
  });

  test('absent timeout emits CLI default (10s)', () => {
    const { out } = convert({ stop: [{ command: 'x' }] });
    expect((out as Record<string, unknown>[])[0]!.timeout).toBe(10);
  });

  test('authored timeout clamps up to at least one second', () => {
    const cases: Array<[number, number]> = [
      [0, 1],
      [1, 1],
      [500, 1],
      [999, 1],
      [1000, 1],
      [1001, 2],
      [2500, 3],
    ];
    for (const [ms, secs] of cases) {
      const { out } = convert({ stop: [{ command: 'x', timeout_ms: ms }] });
      expect((out as Record<string, unknown>[])[0]!.timeout).toBe(secs);
    }
  });

  test('empty object becomes empty array', () => {
    const { out, warnings } = convert({});
    expect(out).toEqual([]);
    expect(warnings).toEqual([]);
  });

  test('array form passes through unchanged', () => {
    const already = [
      {
        name: 'h',
        trigger: 'agentSpawn',
        action: { type: 'command', command: 'x' },
      },
    ];
    const { out, warnings } = convert(already);
    expect(out).toEqual(already);
    expect(warnings).toEqual([]);
  });

  test('tool hook without command is dropped with warning', () => {
    const { out, warnings } = convert({
      postToolUse: [{ tool_name: 'my_tool', args: {} }],
    });
    expect(out).toEqual([]);
    expect(warnings).toEqual(['unconvertible-hook']);
  });

  test('unknown trigger is dropped with warning', () => {
    const { out, warnings } = convert({ postFileSave: [{ command: 'x' }] });
    expect(out).toEqual([]);
    expect(warnings).toEqual(['unconvertible-hook']);
  });

  test('multiple hooks under one trigger get indexed names', () => {
    const { out } = convert({
      agentSpawn: [{ command: 'a' }, { command: 'b' }],
    });
    const names = (out as Record<string, unknown>[]).map((d) => d.name);
    expect(names).toEqual(['agentSpawn-0', 'agentSpawn-1']);
  });

  test('absent / invalid hooks value yields undefined', () => {
    expect(convert(undefined).out).toBeUndefined();
    expect(convert(null).out).toBeUndefined();
    expect(convert('nope').out).toBeUndefined();
  });
});
