import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  AcpRecorder,
  wrapWithRecorder,
  maybeCreateRecorder,
  maybeWrapStreamWithRecorder,
  __resetRecorderForTests,
} from '../acp-recorder';
import type { Stream } from '@kiro/client';

// Minimal message shape for testing; actual ACP messages are richer but the
// recorder treats them opaquely.
type TestMsg = {
  jsonrpc: '2.0';
  id?: number;
  method?: string;
  params?: unknown;
  result?: unknown;
};

function makePair<T>(): {
  stream: Stream;
  push: (msg: T) => void;
  writes: T[];
  close: () => void;
} {
  // Fake the Stream shape: a ReadableStream we control and a WritableStream
  // that captures what the caller writes.
  let readCtrl: ReadableStreamDefaultController<T> | undefined;
  const readable = new ReadableStream<T>({
    start(c) {
      readCtrl = c;
    },
  });
  const writes: T[] = [];
  const writable = new WritableStream<T>({
    write(chunk) {
      writes.push(chunk);
    },
  });
  return {
    stream: { readable, writable } as Stream,
    push: (msg) => readCtrl!.enqueue(msg),
    writes,
    close: () => readCtrl!.close(),
  };
}

function readTrace(
  path: string
): Array<{ ts: number; dir: 'in' | 'out'; msg: unknown }> {
  const contents = readFileSync(path, 'utf-8');
  return contents
    .split('\n')
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l));
}

describe('AcpRecorder', () => {
  let dir: string;
  let tracePath: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'acp-recorder-test-'));
    tracePath = join(dir, 'trace.jsonl');
  });

  afterEach(() => {
    __resetRecorderForTests();
    delete process.env.KIRO_ACP_RECORD_PATH;
    if (existsSync(dir)) rmSync(dir, { recursive: true, force: true });
  });

  it('writes JSONL records with ts, dir, and msg fields', async () => {
    const recorder = new AcpRecorder(tracePath);
    recorder.record('in', { jsonrpc: '2.0', id: 1, result: { ok: true } });
    recorder.record('out', {
      jsonrpc: '2.0',
      id: 2,
      method: 'session/new',
      params: { cwd: '/t' },
    });
    await recorder.close();

    const lines = readTrace(tracePath);
    expect(lines).toHaveLength(2);
    expect(lines[0]!.dir).toBe('in');
    expect(lines[0]!.msg).toEqual({
      jsonrpc: '2.0',
      id: 1,
      result: { ok: true },
    });
    expect(typeof lines[0]!.ts).toBe('number');
    expect(lines[1]!.dir).toBe('out');
    expect((lines[1]!.msg as TestMsg).method).toBe('session/new');
  });

  it('redacts private telemetry identity notifications', async () => {
    const recorder = new AcpRecorder(tracePath);
    recorder.record('in', {
      jsonrpc: '2.0',
      method: '_kiro.dev/telemetry/identityChanged',
      params: { userId: 'private-user-id' },
    });
    await recorder.close();

    const contents = readFileSync(tracePath, 'utf-8');
    expect(contents).not.toContain('private-user-id');
    expect((readTrace(tracePath)[0]!.msg as TestMsg).params).toEqual({
      redacted: true,
    });
  });

  it('appends to existing file on reopen', async () => {
    const first = new AcpRecorder(tracePath);
    first.record('in', { jsonrpc: '2.0', method: 'session/update' });
    await first.close();

    const second = new AcpRecorder(tracePath);
    second.record('out', { jsonrpc: '2.0', id: 1, method: 'initialize' });
    await second.close();

    const lines = readTrace(tracePath);
    expect(lines).toHaveLength(2);
    expect(lines[0]!.dir).toBe('in');
    expect(lines[1]!.dir).toBe('out');
  });

  it('is a no-op after close() (does not throw)', async () => {
    const recorder = new AcpRecorder(tracePath);
    recorder.record('in', { jsonrpc: '2.0', method: 'session/update' });
    await recorder.close();
    // Subsequent record() calls must not throw (recorder may outlive stream).
    recorder.record('in', { jsonrpc: '2.0', method: 'ignored' });
    await recorder.close(); // idempotent
  });

  it('survives failure to open a bad path and swallows subsequent record() calls', async () => {
    // `/nonexistent/nested/dir/trace.jsonl` - createWriteStream will emit an
    // error event async; the recorder should mark itself broken without
    // throwing.
    const badPath = '/nonexistent-kiro-test-dir/trace.jsonl';
    const recorder = new AcpRecorder(badPath);
    // Give the error event a tick to fire.
    await new Promise((r) => setTimeout(r, 10));
    recorder.record('in', { jsonrpc: '2.0', method: 'never-written' });
    await recorder.close();
    expect(existsSync(badPath)).toBe(false);
  });
});

describe('wrapWithRecorder', () => {
  let dir: string;
  let tracePath: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'acp-recorder-test-'));
    tracePath = join(dir, 'trace.jsonl');
  });

  afterEach(() => {
    __resetRecorderForTests();
    delete process.env.KIRO_ACP_RECORD_PATH;
    if (existsSync(dir)) rmSync(dir, { recursive: true, force: true });
  });

  it('passes readable messages through while recording them as `in`', async () => {
    const pair = makePair<TestMsg>();
    const recorder = new AcpRecorder(tracePath);
    const wrapped = wrapWithRecorder(pair.stream, recorder);

    const received: TestMsg[] = [];
    const reader = wrapped.readable.getReader();
    const readAll = (async () => {
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        received.push(value as TestMsg);
      }
    })();

    pair.push({ jsonrpc: '2.0', method: 'session/update', params: { a: 1 } });
    pair.push({ jsonrpc: '2.0', id: 1, result: { ok: true } });
    pair.close();
    await readAll;
    await recorder.close();

    expect(received).toHaveLength(2);
    expect(received[0]!.method).toBe('session/update');
    expect(received[1]!.result).toEqual({ ok: true });

    const trace = readTrace(tracePath);
    expect(trace.every((l) => l.dir === 'in')).toBe(true);
    expect(trace).toHaveLength(2);
  });

  it('forwards writable messages to the underlying stream while recording them as `out`', async () => {
    const pair = makePair<TestMsg>();
    const recorder = new AcpRecorder(tracePath);
    const wrapped = wrapWithRecorder(pair.stream, recorder);

    const writer = wrapped.writable.getWriter();
    await writer.write({
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {},
    });
    await writer.write({
      jsonrpc: '2.0',
      id: 2,
      method: 'session/new',
      params: { cwd: '/t' },
    });
    writer.releaseLock();
    await recorder.close();

    // Forwarded to underlying writable?
    expect(pair.writes).toHaveLength(2);
    expect(pair.writes[0]!.method).toBe('initialize');
    expect(pair.writes[1]!.method).toBe('session/new');

    // Recorded as `out`?
    const trace = readTrace(tracePath);
    expect(trace.every((l) => l.dir === 'out')).toBe(true);
    expect(trace).toHaveLength(2);
  });

  it('captures bidirectional traffic in the order it flows', async () => {
    const pair = makePair<TestMsg>();
    const recorder = new AcpRecorder(tracePath);
    const wrapped = wrapWithRecorder(pair.stream, recorder);

    const reader = wrapped.readable.getReader();
    const writer = wrapped.writable.getWriter();

    // Client sends initialize.
    await writer.write({
      jsonrpc: '2.0',
      id: 1,
      method: 'initialize',
      params: {},
    });
    // Agent responds.
    pair.push({ jsonrpc: '2.0', id: 1, result: { protocolVersion: 1 } });
    const r1 = await reader.read();
    expect((r1.value as TestMsg).result).toEqual({ protocolVersion: 1 });
    // Agent pushes a notification.
    pair.push({
      jsonrpc: '2.0',
      method: 'session/update',
      params: { u: true },
    });
    const r2 = await reader.read();
    expect((r2.value as TestMsg).method).toBe('session/update');
    // Client sends another request.
    await writer.write({
      jsonrpc: '2.0',
      id: 2,
      method: 'session/new',
      params: {},
    });

    pair.close();
    writer.releaseLock();
    await recorder.close();

    const trace = readTrace(tracePath);
    const dirs = trace.map((l) => l.dir);
    expect(dirs).toEqual(['out', 'in', 'in', 'out']);
  });
});

describe('maybeCreateRecorder', () => {
  let dir: string;
  let tracePath: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'acp-recorder-test-'));
    tracePath = join(dir, 'trace.jsonl');
  });

  afterEach(() => {
    __resetRecorderForTests();
    delete process.env.KIRO_ACP_RECORD_PATH;
    if (existsSync(dir)) rmSync(dir, { recursive: true, force: true });
  });

  it('returns null when KIRO_ACP_RECORD_PATH is unset', () => {
    delete process.env.KIRO_ACP_RECORD_PATH;
    expect(maybeCreateRecorder()).toBeNull();
  });

  it('returns a recorder when KIRO_ACP_RECORD_PATH is set', () => {
    process.env.KIRO_ACP_RECORD_PATH = tracePath;
    const r = maybeCreateRecorder();
    expect(r).not.toBeNull();
  });

  it('returns the same instance on repeated calls (singleton)', () => {
    process.env.KIRO_ACP_RECORD_PATH = tracePath;
    const a = maybeCreateRecorder();
    const b = maybeCreateRecorder();
    expect(a).toBe(b);
  });

  it('__resetRecorderForTests removes the signal handlers it registered', () => {
    process.env.KIRO_ACP_RECORD_PATH = tracePath;
    const before = {
      sigint: process.listenerCount('SIGINT'),
      sigterm: process.listenerCount('SIGTERM'),
      beforeExit: process.listenerCount('beforeExit'),
    };
    // Create and reset five times; listener counts must return to baseline
    // each cycle so real test runs don't hit Node's MaxListeners warning.
    for (let i = 0; i < 5; i++) {
      maybeCreateRecorder();
      __resetRecorderForTests();
    }
    expect(process.listenerCount('SIGINT')).toBe(before.sigint);
    expect(process.listenerCount('SIGTERM')).toBe(before.sigterm);
    expect(process.listenerCount('beforeExit')).toBe(before.beforeExit);
  });
});

describe('maybeWrapStreamWithRecorder', () => {
  let dir: string;
  let tracePath: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), 'acp-recorder-test-'));
    tracePath = join(dir, 'trace.jsonl');
  });

  afterEach(() => {
    __resetRecorderForTests();
    delete process.env.KIRO_ACP_RECORD_PATH;
    if (existsSync(dir)) rmSync(dir, { recursive: true, force: true });
  });

  it('returns the input stream unchanged when the env var is unset', () => {
    delete process.env.KIRO_ACP_RECORD_PATH;
    const pair = makePair<TestMsg>();
    const wrapped = maybeWrapStreamWithRecorder(pair.stream);
    expect(wrapped).toBe(pair.stream);
  });

  it('wraps the stream with a recorder when the env var is set', async () => {
    process.env.KIRO_ACP_RECORD_PATH = tracePath;
    const pair = makePair<TestMsg>();
    const wrapped = maybeWrapStreamWithRecorder(pair.stream);
    expect(wrapped).not.toBe(pair.stream);
    // Sanity-check wiring: a write reaches the underlying stream + trace.
    const writer = wrapped.writable.getWriter();
    await writer.write({ jsonrpc: '2.0', id: 1, method: 'ping' });
    writer.releaseLock();
    expect(pair.writes).toHaveLength(1);
  });
});
