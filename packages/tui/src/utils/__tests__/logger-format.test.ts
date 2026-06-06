/**
 * Unit tests for formatArg — the helper that serializes log args.
 *
 * Specifically guards the Error-instance branch: `JSON.stringify(new Error())`
 * returns "{}" because message/stack/name are non-enumerable, which made
 * every `[store] sendMessage: caught error {}` log line in kiro-tui.log
 * useless for debugging. formatArg pulls those fields off explicitly.
 */

import { describe, it, expect } from 'bun:test';
import { formatArg } from '../logger';

describe('formatArg', () => {
  it('serializes plain Error with name and message', () => {
    const out = formatArg(new Error('boom'));
    const parsed = JSON.parse(out);
    expect(parsed.name).toBe('Error');
    expect(parsed.message).toBe('boom');
    expect(typeof parsed.stack).toBe('string');
  });

  it('serializes custom Error subclass with its name', () => {
    class MyError extends Error {
      constructor(msg: string) {
        super(msg);
        this.name = 'MyError';
      }
    }
    const parsed = JSON.parse(formatArg(new MyError('custom')));
    expect(parsed.name).toBe('MyError');
    expect(parsed.message).toBe('custom');
  });

  it('includes .code when present', () => {
    const err: any = new Error('enoent');
    err.code = 'ENOENT';
    const parsed = JSON.parse(formatArg(err));
    expect(parsed.code).toBe('ENOENT');
  });

  it('recursively serializes .cause when present', () => {
    const inner = new Error('inner');
    const outer = new Error('outer', { cause: inner });
    const parsed = JSON.parse(formatArg(outer));
    expect(parsed.message).toBe('outer');
    expect(typeof parsed.cause).toBe('string');
    const innerParsed = JSON.parse(parsed.cause);
    expect(innerParsed.message).toBe('inner');
    expect(innerParsed.name).toBe('Error');
  });

  it('serializes plain objects via JSON.stringify', () => {
    expect(formatArg({ a: 1, b: 'x' })).toBe('{"a":1,"b":"x"}');
  });

  it('serializes arrays via JSON.stringify', () => {
    expect(formatArg([1, 2, 3])).toBe('[1,2,3]');
  });

  it('falls back to String() when JSON.stringify throws (circular)', () => {
    const circular: any = {};
    circular.self = circular;
    const out = formatArg(circular);
    // Either the fallback "[object Object]" or some safe stringification —
    // the contract is "doesn't throw".
    expect(typeof out).toBe('string');
    expect(out.length).toBeGreaterThan(0);
  });

  it('passes primitives through String()', () => {
    expect(formatArg(42)).toBe('42');
    expect(formatArg('hello')).toBe('hello');
    expect(formatArg(true)).toBe('true');
    expect(formatArg(null)).toBe('null');
    expect(formatArg(undefined)).toBe('undefined');
  });

  it('regression: Error no longer serializes to "{}"', () => {
    // The whole reason this helper exists.
    const out = formatArg(new Error('not empty'));
    expect(out).not.toBe('{}');
    expect(out).toContain('not empty');
  });
});
