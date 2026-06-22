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

class MyError extends Error {
  constructor(msg: string) {
    super(msg);
    this.name = 'MyError';
  }
}
const withCode = (msg: string, code: string) => {
  const err: any = new Error(msg);
  err.code = code;
  return err;
};

describe('formatArg', () => {
  // The non-enumerable Error fields the helper must pull off explicitly: name,
  // message, stack, .code, and a recursively-serialized .cause. The plain-Error
  // case also proves the regression that JSON.stringify(Error) === "{}".
  it.each([
    {
      name: 'plain Error (name/message/stack, not "{}")',
      err: new Error('boom'),
      expected: { name: 'Error', message: 'boom' },
    },
    {
      name: 'custom Error subclass keeps its name',
      err: new MyError('custom'),
      expected: { name: 'MyError', message: 'custom' },
    },
    {
      name: 'includes .code when present',
      err: withCode('enoent', 'ENOENT'),
      expected: { message: 'enoent', code: 'ENOENT' },
    },
  ])('serializes $name', ({ err, expected }) => {
    const out = formatArg(err);
    expect(out).not.toBe('{}');
    const parsed = JSON.parse(out);
    expect(typeof parsed.stack).toBe('string');
    expect(parsed).toMatchObject(expected);
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

  it.each([
    [{ a: 1, b: 'x' }, '{"a":1,"b":"x"}'],
    [[1, 2, 3], '[1,2,3]'],
  ])('serializes %j via JSON.stringify', (value, expected) => {
    expect(formatArg(value)).toBe(expected);
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

  it.each([
    [42, '42'],
    ['hello', 'hello'],
    [true, 'true'],
    [null, 'null'],
    [undefined, 'undefined'],
  ])('passes primitive %p through String()', (value, expected) => {
    expect(formatArg(value)).toBe(expected);
  });
});
