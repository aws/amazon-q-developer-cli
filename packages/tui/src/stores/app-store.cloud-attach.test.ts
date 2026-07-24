import { afterAll, beforeAll, describe, expect, it, mock } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { Kiro } from '../kiro.js';
import { createAppStore } from './app-store.js';

let directory: string;
let directPath: string;
let nestedPath: string;
let wrapperPath: string;

beforeAll(() => {
  directory = mkdtempSync(join(tmpdir(), 'kiro-cloud-store-'));
  directPath = join(directory, 'direct.txt');
  nestedPath = join(directory, 'nested.txt');
  wrapperPath = join(directory, 'wrapper.txt');
  writeFileSync(directPath, 'direct contents');
  writeFileSync(nestedPath, 'nested contents');
  writeFileSync(wrapperPath, `configuration references ${nestedPath}`);
});

afterAll(() => rmSync(directory, { recursive: true, force: true }));

function createStore(cloudActive: boolean) {
  const streamMessage = mock(
    (..._args: Parameters<Kiro['streamMessage']>): Promise<void> =>
      Promise.resolve()
  );
  const kiro = {
    isCloudSessionActive: () => cloudActive,
    streamMessage,
  } as unknown as Kiro;
  const store = createAppStore({ kiro });
  store.setState({ isInitialized: true });
  return { store, streamMessage };
}

describe('sendMessage cloud attachments', () => {
  it('forwards direct local paths only for active cloud sessions', async () => {
    const cloud = createStore(true);
    await cloud.store.getState().sendMessage(`read ${directPath}`);

    const cloudArgs = cloud.streamMessage.mock.calls[0]!;
    expect(cloudArgs[4]?.[0]).toMatchObject({
      uri: pathToFileURL(directPath).href,
      text: 'direct contents',
    });

    const local = createStore(false);
    await local.store.getState().sendMessage(`read ${directPath}`);
    expect(local.streamMessage.mock.calls[0]![4]).toBeUndefined();
  });

  it('uses display content when dispatching strips a leading slash', async () => {
    const { store, streamMessage } = createStore(true);
    await store
      .getState()
      .sendMessage(directPath.slice(1), undefined, directPath);

    expect(streamMessage.mock.calls[0]![4]?.[0]?.uri).toBe(
      pathToFileURL(directPath).href
    );
  });

  it('does not rescan paths from expanded file contents', async () => {
    const { store, streamMessage } = createStore(true);
    await store.getState().sendMessage(`@file:${wrapperPath}`);

    const args = streamMessage.mock.calls[0]!;
    expect(args[0]).toContain(`configuration references ${nestedPath}`);
    expect(args[4]).toBeUndefined();
    expect(args[5]).toBeUndefined();
  });
});
