import { describe, it, expect } from 'bun:test';
import { encodeFrame, FrameDecoder } from '../test-utils/acp-mock/framing';

describe('encodeFrame', () => {
  it('serializes and appends a newline', () => {
    expect(encodeFrame({ hello: 'world' })).toBe('{"hello":"world"}\n');
  });

  it('handles primitive values', () => {
    expect(encodeFrame(42)).toBe('42\n');
    expect(encodeFrame('x')).toBe('"x"\n');
    expect(encodeFrame(null)).toBe('null\n');
  });

  it('throws on circular references', () => {
    const a: Record<string, unknown> = {};
    a.self = a;
    expect(() => encodeFrame(a)).toThrow();
  });
});

describe('FrameDecoder', () => {
  it('yields messages split on newlines', () => {
    const d = new FrameDecoder();
    expect(d.feed('{"a":1}\n{"b":2}\n')).toEqual([{ a: 1 }, { b: 2 }]);
  });

  it('holds back a partial line until the rest arrives', () => {
    const d = new FrameDecoder();
    expect(d.feed('{"a":1}\n{"b":')).toEqual([{ a: 1 }]);
    expect(d.feed('2}\n')).toEqual([{ b: 2 }]);
  });

  it('handles chunks that split between messages cleanly', () => {
    const d = new FrameDecoder();
    expect(d.feed('{"a"')).toEqual([]);
    expect(d.feed(':1}')).toEqual([]);
    expect(d.feed('\n')).toEqual([{ a: 1 }]);
  });

  it('skips blank lines', () => {
    const d = new FrameDecoder();
    expect(d.feed('\n\n{"a":1}\n\n')).toEqual([{ a: 1 }]);
  });

  it('reports malformed lines via onError and continues', () => {
    const d = new FrameDecoder();
    const errors: Array<{ line: string }> = [];
    const result = d.feed('{bad}\n{"a":1}\n', (line) => errors.push({ line }));
    expect(result).toEqual([{ a: 1 }]);
    expect(errors).toHaveLength(1);
    expect(errors[0]!.line).toBe('{bad}');
  });

  it('finalize() drains a trailing partial message without newline', () => {
    const d = new FrameDecoder();
    d.feed('{"a":1}\n{"b":2}');
    expect(d.finalize()).toEqual([{ b: 2 }]);
  });

  it('finalize() is empty when buffer is empty', () => {
    const d = new FrameDecoder();
    d.feed('{"a":1}\n');
    expect(d.finalize()).toEqual([]);
  });

  it('finalize() reports malformed trailing content and returns nothing', () => {
    const d = new FrameDecoder();
    const errors: Array<{ line: string }> = [];
    d.feed('{"a":1}\n{bad');
    expect(d.finalize((line) => errors.push({ line }))).toEqual([]);
    expect(errors).toHaveLength(1);
    expect(errors[0]!.line).toBe('{bad');
  });

  it('roundtrips encodeFrame output through FrameDecoder', () => {
    const msgs = [
      { jsonrpc: '2.0', id: 1, method: 'initialize' },
      { jsonrpc: '2.0', method: 'session/update', params: { a: 1 } },
      { jsonrpc: '2.0', id: 1, result: { ok: true } },
    ];
    const wire = msgs.map(encodeFrame).join('');
    // Split into arbitrary chunks to simulate real socket behavior.
    const chunks = [wire.slice(0, 10), wire.slice(10, 25), wire.slice(25)];
    const d = new FrameDecoder();
    const out = chunks.flatMap((c) => d.feed(c));
    expect(out).toEqual(msgs);
  });
});
