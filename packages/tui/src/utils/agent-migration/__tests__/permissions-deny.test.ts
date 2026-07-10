/**
 * Focused unit tests for the allow/deny asymmetry in permission conversion:
 * an unconvertible ALLOW is dropped (never fabricate an over-broad grant), but
 * an unconvertible DENY must fail closed — widen to deny-all — so an explicitly
 * blocked command can never silently become allowed after migration.
 */

import { describe, test, expect } from 'bun:test';

import { convertPatterns, convertToolsSettings } from '../permissions.js';
import type { MigrationWarning } from '../permissions.js';

describe('convertPatterns — allow vs deny on unconvertible', () => {
  test('unconvertible ALLOW emits nothing (+ warning)', () => {
    const warnings: MigrationWarning[] = [];
    const out = convertPatterns(
      ['(?=.*--force).*rm.*'],
      'shell',
      warnings,
      'allow'
    );
    expect(out).toEqual([]);
    expect(warnings.map((w) => w.kind)).toEqual(['unconvertible-pattern']);
  });

  test('unconvertible DENY fails closed to `*` (+ warning)', () => {
    const warnings: MigrationWarning[] = [];
    const out = convertPatterns(
      ['(?=.*--force).*rm.*'],
      'shell',
      warnings,
      'deny'
    );
    expect(out).toEqual(['*']);
    expect(warnings.map((w) => w.kind)).toEqual(['unconvertible-pattern']);
  });

  test('deny with one convertible + one unconvertible → deny-all wins', () => {
    const warnings: MigrationWarning[] = [];
    const out = convertPatterns(
      ['git push .*', '(?=.*Admin).*'],
      'shell',
      warnings,
      'deny'
    );
    expect(out).toEqual(['git push *', '*']);
  });
});

describe('convertToolsSettings — deniedCommands never vanish', () => {
  test('unconvertible deniedCommands produce a deny-all shell rule', () => {
    const warnings: MigrationWarning[] = [];
    const perms = convertToolsSettings(
      { execute_bash: { deniedCommands: ['.*&&.*rm -rf.*'] } },
      warnings
    );
    const denyRules = (perms?.rules ?? []).filter((r) => r.effect === 'deny');
    expect(denyRules).toEqual([
      { capability: 'shell', match: ['*'], effect: 'deny' },
    ]);
    expect(warnings.some((w) => w.kind === 'unconvertible-pattern')).toBe(true);
  });
});
