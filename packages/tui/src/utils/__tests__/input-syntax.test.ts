import { describe, test, expect } from 'vitest';
import { computeInputSpans } from '../input-syntax.js';

describe('computeInputSpans', () => {
  test('empty input returns no spans', () => {
    expect(computeInputSpans('')).toEqual([]);
  });

  test('plain prose returns no spans', () => {
    expect(computeInputSpans('hello there what is up')).toEqual([]);
  });

  test('absolute path is detected', () => {
    const spans = computeInputSpans('open /etc/hosts please');
    expect(spans).toHaveLength(1);
    expect(spans[0]).toMatchObject({ kind: 'path' });
    expect('open /etc/hosts please'.slice(spans[0]!.start, spans[0]!.end)).toBe(
      '/etc/hosts'
    );
  });

  test('relative path with ./ is detected', () => {
    const spans = computeInputSpans('check ./src/foo.ts now');
    expect(spans).toHaveLength(1);
    expect(spans[0]!.kind).toBe('path');
  });

  test('home path with ~/ is detected', () => {
    const spans = computeInputSpans('cd ~/projects/kiro');
    expect(spans).toHaveLength(1);
    expect(spans[0]!.kind).toBe('path');
  });

  test('slash command at start is detected when in known set', () => {
    const known = new Set(['/help']);
    const spans = computeInputSpans('/help me', known);
    expect(spans[0]).toMatchObject({ kind: 'slash', start: 0, end: 5 });
  });

  test('slash command is NOT highlighted when knownCommands is omitted', () => {
    // Without an explicit known set, we treat leading `/word` conservatively
    // so a path like `/tmp/foo` doesn't get its first segment recolored. The
    // path regex below picks up the full path instead.
    const spans = computeInputSpans('/help me');
    expect(spans.every((s) => s.kind !== 'slash')).toBe(true);
  });

  test('unknown slash word is not styled as a command', () => {
    const known = new Set(['/help', '/spec']);
    const spans = computeInputSpans('/notacommand foo', known);
    expect(spans.every((s) => s.kind !== 'slash')).toBe(true);
  });

  test('slash command does NOT match a path-like first token', () => {
    // `/abs` is not a known command, so the leading /abs is left alone and
    // the path regex picks up the full `/abs/path/to/file` as a path span.
    const known = new Set(['/help']);
    const spans = computeInputSpans('/abs/path/to/file', known);
    expect(spans).toHaveLength(1);
    expect(spans[0]!.kind).toBe('path');
  });

  test('path that starts with a known command name is still a path', () => {
    // `/help/foo` looks like the `/help` command but the trailing `/foo`
    // means it's actually a path. We should not split it into command +
    // path — color the whole token as a path.
    const known = new Set(['/help']);
    const spans = computeInputSpans('/help/foo', known);
    expect(spans).toHaveLength(1);
    expect(spans[0]!.kind).toBe('path');
  });

  test('URL is detected', () => {
    const text = 'see https://example.com/docs for more';
    const spans = computeInputSpans(text);
    const url = spans.find((s) => s.kind === 'url');
    expect(url).toBeDefined();
    expect(text.slice(url!.start, url!.end)).toBe('https://example.com/docs');
  });

  test('URL takes precedence over path-looking trailing /', () => {
    const text = 'fetch https://x.com/path/file.txt now';
    const spans = computeInputSpans(text);
    expect(spans).toHaveLength(1);
    expect(spans[0]!.kind).toBe('url');
  });

  test('multiple paths are all returned in order', () => {
    const text = 'compare ./a.ts and ./b.ts files';
    const spans = computeInputSpans(text);
    expect(spans).toHaveLength(2);
    expect(spans[0]!.start).toBeLessThan(spans[1]!.start);
    expect(spans.every((s) => s.kind === 'path')).toBe(true);
  });

  test('lone "/" is not a path', () => {
    expect(computeInputSpans('what is / used for')).toEqual([]);
  });
});
