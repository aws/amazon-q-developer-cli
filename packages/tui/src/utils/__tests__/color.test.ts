import { describe, expect, test } from 'bun:test';
import { join } from 'path';
import { resolveForcedLevel } from '../color';

describe('resolveForcedLevel', () => {
  test('returns the parsed level from KIRO_TUI_FORCE_COLOR', () => {
    expect(resolveForcedLevel({ KIRO_TUI_FORCE_COLOR: '0' })).toBe(0);
    expect(resolveForcedLevel({ KIRO_TUI_FORCE_COLOR: '1' })).toBe(1);
    expect(resolveForcedLevel({ KIRO_TUI_FORCE_COLOR: '2' })).toBe(2);
    expect(resolveForcedLevel({ KIRO_TUI_FORCE_COLOR: '3' })).toBe(3);
  });

  test('returns undefined when the variable is unset', () => {
    expect(resolveForcedLevel({})).toBeUndefined();
  });

  test('returns undefined for out-of-range or non-numeric values', () => {
    expect(resolveForcedLevel({ KIRO_TUI_FORCE_COLOR: '4' })).toBeUndefined();
    expect(resolveForcedLevel({ KIRO_TUI_FORCE_COLOR: '-1' })).toBeUndefined();
    expect(
      resolveForcedLevel({ KIRO_TUI_FORCE_COLOR: 'true' })
    ).toBeUndefined();
  });

  test('never reads or writes FORCE_COLOR', () => {
    const env: NodeJS.ProcessEnv = { KIRO_TUI_FORCE_COLOR: '3' };
    resolveForcedLevel(env);
    expect(env.FORCE_COLOR).toBeUndefined();
  });
});

describe('shared chalk instance (startup)', () => {
  const fixture = join(import.meta.dir, 'fixtures', 'color-instance.ts');

  const runFixture = (extraEnv: Record<string, string>) => {
    const env: Record<string, string> = { PATH: process.env.PATH ?? '' };
    Object.assign(env, extraEnv);
    const result = Bun.spawnSync(['bun', 'run', fixture], { env });
    expect(result.exitCode).toBe(0);
    return JSON.parse(result.stdout.toString()) as {
      level: number;
      forceColor: string | null;
    };
  };

  test('pins chalk level from the private variable without setting FORCE_COLOR', () => {
    const result = runFixture({ KIRO_TUI_FORCE_COLOR: '3' });
    expect(result.level).toBe(3);
    expect(result.forceColor).toBeNull();
  });

  test('auto-detects (level 0 on a pipe) when the variable is unset', () => {
    const result = runFixture({});
    expect(result.level).toBe(0);
    expect(result.forceColor).toBeNull();
  });
});
