/**
 * Unit tests for `refresh-feed-cli`. This wrapper contributes argv
 * assembly and result narrowing on top of `chat-internal-cli`;
 * shared-layer behavior is exercised in `chat-internal-cli.test.ts`.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { refreshChangelogFeed } from '../refresh-feed-cli';
import type { AsyncSpawner } from '../chat-internal-cli';

interface SpawnerHarness {
  spawner: AsyncSpawner;
  calls: Array<{ cmd: string; args: string[] }>;
}

function makeSpawner(stdout: string, status = 0): SpawnerHarness {
  const calls: SpawnerHarness['calls'] = [];
  const spawner: AsyncSpawner = async (cmd, args) => {
    calls.push({ cmd, args });
    return { status, stdout, stderr: '' };
  };
  return { spawner, calls };
}

const FAKE_BIN = '/fake/chat_cli';
let originalBin: string | undefined;

beforeEach(() => {
  originalBin = process.env.KIRO_CHAT_CLI_BIN;
  process.env.KIRO_CHAT_CLI_BIN = FAKE_BIN;
});

afterEach(() => {
  if (originalBin === undefined) delete process.env.KIRO_CHAT_CLI_BIN;
  else process.env.KIRO_CHAT_CLI_BIN = originalBin;
});

describe('refreshChangelogFeed', () => {
  it('invokes chat _ refresh-feed and resolves true when updated', async () => {
    const { spawner, calls } = makeSpawner(
      JSON.stringify({ kind: 'refreshFeed', data: { updated: true } })
    );
    expect(await refreshChangelogFeed(spawner)).toBe(true);
    expect(calls).toHaveLength(1);
    expect(calls[0]!.cmd).toBe(FAKE_BIN);
    expect(calls[0]!.args).toEqual(['chat', '_', 'refresh-feed']);
  });

  it('resolves false when the fetch did not update the feed', async () => {
    const { spawner } = makeSpawner(
      JSON.stringify({ kind: 'refreshFeed', data: { updated: false } })
    );
    expect(await refreshChangelogFeed(spawner)).toBe(false);
  });

  it('resolves false on the error variant', async () => {
    const { spawner } = makeSpawner(
      JSON.stringify({ kind: 'error', data: { message: 'boom' } }),
      1
    );
    expect(await refreshChangelogFeed(spawner)).toBe(false);
  });

  it('resolves false when the spawn itself fails', async () => {
    const spawner: AsyncSpawner = async () => ({
      status: null,
      stdout: '',
      stderr: '',
      error: new Error('ENOENT'),
    });
    expect(await refreshChangelogFeed(spawner)).toBe(false);
  });

  it('does not spawn when the signal is already aborted', async () => {
    const { spawner, calls } = makeSpawner(
      JSON.stringify({ kind: 'refreshFeed', data: { updated: true } })
    );
    const controller = new AbortController();
    controller.abort();
    expect(await refreshChangelogFeed(spawner, controller.signal)).toBe(false);
    expect(calls).toHaveLength(0);
  });

  it('aborting mid-flight kills the spawn via the passed signal', async () => {
    let aborted = false;
    const spawner: AsyncSpawner = (_cmd, _args, options) =>
      new Promise((resolve) => {
        options?.signal?.addEventListener('abort', () => {
          aborted = true;
          resolve({ status: null, stdout: '', stderr: '' });
        });
      });
    const controller = new AbortController();
    const promise = refreshChangelogFeed(spawner, controller.signal);
    controller.abort();
    expect(await promise).toBe(false);
    expect(aborted).toBe(true);
  });
});
