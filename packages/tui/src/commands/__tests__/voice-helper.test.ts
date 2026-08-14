import { afterAll, beforeEach, describe, expect, it, mock } from 'bun:test';
import { EventEmitter } from 'events';

interface MockChild extends EventEmitter {
  stdin: {
    write: ReturnType<typeof mock>;
    end: ReturnType<typeof mock>;
  };
  stdout: EventEmitter;
  stderr: EventEmitter;
  kill: ReturnType<typeof mock>;
}

function createMockChild(): MockChild {
  const child = new EventEmitter() as MockChild;
  child.stdin = {
    write: mock(() => true),
    end: mock(() => {}),
  };
  child.stdout = new EventEmitter();
  child.stderr = new EventEmitter();
  child.kill = mock(() => true);
  return child;
}

let child: MockChild;
const mockSpawn = mock(() => child);

mock.module('child_process', () => ({ spawn: mockSpawn }));

const { startPTTRecording } = await import('../voice-helper');

beforeEach(() => {
  child = createMockChild();
  mockSpawn.mockClear();
});

afterAll(() => {
  mock.restore();
});

describe('startPTTRecording', () => {
  it('rejects when the helper exits unsuccessfully without a structured error', async () => {
    const session = startPTTRecording();

    child.emit('close', 2, null);

    await expect(session.text).rejects.toThrow(
      'Voice helper exited with code 2'
    );
  });

  it('resolves no speech only after a successful empty transcription', async () => {
    const session = startPTTRecording();

    child.emit('close', 0, null);

    await expect(session.text).resolves.toBeNull();
  });

  it('resolves cancellation without surfacing the termination signal', async () => {
    const session = startPTTRecording();

    session.cancel();
    child.emit('close', null, 'SIGKILL');

    expect(child.kill).toHaveBeenCalledWith('SIGKILL');
    await expect(session.text).resolves.toBeNull();
  });

  it('preserves structured helper errors', async () => {
    const session = startPTTRecording();
    child.stdout.emit(
      'data',
      Buffer.from(
        '{"type":"error","value":{"message":"model download failed"}}\n'
      )
    );

    child.emit('close', 1, null);

    await expect(session.text).rejects.toThrow('model download failed');
  });

  it('returns text emitted by a successful helper', async () => {
    const session = startPTTRecording();
    child.stdout.emit('data', Buffer.from('{"type":"text","value":"hello"}\n'));

    child.emit('close', 0, null);

    await expect(session.text).resolves.toBe('hello');
  });
});
