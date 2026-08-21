import { afterAll, beforeAll, describe, expect, it, mock } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { Kiro } from '../kiro.js';
import { InterruptMode } from '../constants/interrupt-mode.js';
import { createAppStore } from './app-store.js';

let directory: string;
let directPath: string;
let nestedPath: string;
let wrapperPath: string;
let imagePath: string;

const TINY_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64'
);

beforeAll(() => {
  directory = mkdtempSync(join(tmpdir(), 'kiro-attachments-store-'));
  directPath = join(directory, 'direct.txt');
  nestedPath = join(directory, 'nested.txt');
  wrapperPath = join(directory, 'wrapper.txt');
  imagePath = join(directory, 'shot.png');
  writeFileSync(directPath, 'direct contents');
  writeFileSync(nestedPath, 'nested contents');
  writeFileSync(wrapperPath, `configuration references ${nestedPath}`);
  writeFileSync(imagePath, TINY_PNG);
});

afterAll(() => rmSync(directory, { recursive: true, force: true }));

function createStore(cloudActive: boolean) {
  const streamMessage = mock(
    (..._args: Parameters<Kiro['streamMessage']>): Promise<void> =>
      Promise.resolve()
  );
  const steerMessage = mock((): Promise<void> => Promise.resolve());
  const kiro = {
    isCloudSessionActive: () => cloudActive,
    streamMessage,
    steerMessage,
  } as unknown as Kiro;
  const store = createAppStore({ kiro });
  store.setState({ isInitialized: true });
  return { store, streamMessage, steerMessage };
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

describe('sendMessage image attachments', () => {
  /** streamMessage args: [content, signal, handler, images, resources, blobs]. */
  it('attaches a referenced image in an ordinary session', async () => {
    const { store, streamMessage } = createStore(false);
    await store.getState().sendMessage(`describe ${imagePath}`);

    const images = streamMessage.mock.calls[0]![3];
    expect(images).toHaveLength(1);
    expect(images![0]).toMatchObject({
      mimeType: 'image/png',
      base64: TINY_PNG.toString('base64'),
    });
  });

  it('keeps text and documents out of an ordinary session', async () => {
    const { store, streamMessage } = createStore(false);
    await store.getState().sendMessage(`read ${directPath}`);

    const args = streamMessage.mock.calls[0]!;
    expect(args[3]).toBeUndefined();
    expect(args[4]).toBeUndefined();
    expect(args[5]).toBeUndefined();
  });

  it('attaches a referenced image in a cloud session too', async () => {
    const { store, streamMessage } = createStore(true);
    await store.getState().sendMessage(`describe ${imagePath}`);

    expect(streamMessage.mock.calls[0]![3]).toHaveLength(1);
  });

  it('sends no attachments when no path is referenced', async () => {
    const { store, streamMessage } = createStore(false);
    await store.getState().sendMessage('just a question');

    expect(streamMessage.mock.calls[0]![3]).toBeUndefined();
  });

  it('merges a referenced image with an already-pasted image', async () => {
    const { store, streamMessage } = createStore(false);
    store.getState().addPendingImage({
      base64: 'cGFzdGVk',
      mimeType: 'image/png',
      width: 1,
      height: 1,
      sizeBytes: 6,
    });

    await store.getState().sendMessage(`compare with ${imagePath}`);

    const images = streamMessage.mock.calls[0]![3];
    expect(images?.map((image) => image.base64)).toEqual([
      'cGFzdGVk',
      TINY_PNG.toString('base64'),
    ]);
  });
});

describe('sendMessage display content', () => {
  it('echoes the display form while sending the path to the model', async () => {
    // What the prompt does with an image chip: the model gets the path, the
    // transcript gets a name. Nothing else has to stay in sync for the image
    // to arrive, because the path in the content is what attaches it.
    const { store, streamMessage } = createStore(false);
    await store
      .getState()
      .sendMessage(
        `describe ${imagePath}`,
        undefined,
        'describe [image: shot.png]'
      );

    const args = streamMessage.mock.calls[0]!;
    expect(args[0]).toContain(imagePath);
    expect(args[3]).toHaveLength(1);

    const shown = store.getState().messages.at(-1);
    expect(shown?.content).toBe('describe [image: shot.png]');
    expect(shown?.content).not.toContain(imagePath);
  });

  it('attaches nothing once the path is gone, which is how a removed chip detaches', async () => {
    const { store, streamMessage } = createStore(false);
    await store.getState().sendMessage('describe');

    expect(streamMessage.mock.calls[0]![3]).toBeUndefined();
  });
});

describe('a prompt that is only an image chip', () => {
  it('is sent as a message, not parsed as a slash command', async () => {
    // The chip's content is an absolute path, so it leads with "/". Routed as a
    // command, the leading slash is stripped and the path no longer resolves.
    const { store, streamMessage } = createStore(false);
    await store
      .getState()
      .handleUserInput(imagePath, 'user', `[image: ${basename(imagePath)}]`);

    const args = streamMessage.mock.calls[0]!;
    expect(args[0]).toBe(imagePath);
    expect(args[3]).toHaveLength(1);
  });

  it('shows the chip label rather than the path', async () => {
    const { store } = createStore(false);
    await store
      .getState()
      .handleUserInput(imagePath, 'user', `[image: ${basename(imagePath)}]`);

    const shown = store.getState().messages.at(-1);
    expect(shown?.content).toBe('[image: shot.png]');
    expect(shown?.content).not.toContain(imagePath);
  });

  it('still routes a real slash command when no chip is present', async () => {
    const { store, streamMessage } = createStore(false);
    await store.getState().handleUserInput(imagePath);

    // Without a chip this is the pre-existing typed-path behaviour: the command
    // parser strips the leading slash, and the path survives via display text.
    const args = streamMessage.mock.calls[0]!;
    expect(args[0]).not.toBe(imagePath);
    expect(args[3]).toHaveLength(1);
  });
});

describe('an image chip submitted while a turn is running', () => {
  const midTurnSteerStore = (cloudActive = false) => {
    const created = createStore(cloudActive);
    created.store.setState({
      sessionId: 'session-1',
      activeInterruptMode: InterruptMode.STEER,
      isProcessing: true,
    });
    return created;
  };

  it('waits in the queue instead of steering, which carries text only', async () => {
    const { store, steerMessage } = midTurnSteerStore();

    await store
      .getState()
      .handleUserInput(
        `describe ${imagePath}`,
        'user',
        'describe [image: shot.png]',
        true
      );

    expect(steerMessage).not.toHaveBeenCalled();
    expect(store.getState().queuedMessages).toEqual([`describe ${imagePath}`]);
  });

  it('sends the image bytes once the queue drains', async () => {
    const { store, streamMessage } = midTurnSteerStore();
    await store
      .getState()
      .handleUserInput(`describe ${imagePath}`, 'user', undefined, true);

    store.setState({ isProcessing: false });
    await store.getState().processQueue();

    const args = streamMessage.mock.calls[0]!;
    expect(args[0]).toContain(imagePath);
    expect(args[3]).toHaveLength(1);
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('still steers a message with no attachment', async () => {
    const { store, steerMessage } = midTurnSteerStore();

    await store.getState().handleUserInput('actually, stop');

    expect(steerMessage).toHaveBeenCalledWith('session-1', 'actually, stop');
    expect(store.getState().queuedMessages).toEqual([]);
  });

  it('queues a chip-only prompt rather than reading its path as a command', async () => {
    // A cloud session cannot probe the path to rule out a command, so without
    // the attachment signal this leading-slash prompt is reported as a typo.
    const { store } = midTurnSteerStore(true);

    await store
      .getState()
      .handleUserInput(
        imagePath,
        'user',
        `[image: ${basename(imagePath)}]`,
        true
      );

    expect(store.getState().queuedMessages).toEqual([imagePath]);
    expect(store.getState().transientAlert).toBeNull();
  });
});
