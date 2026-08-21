import { afterAll, afterEach, beforeAll, describe, expect, it } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const SESSION_ID = 'image-attachment-session';

const TINY_PNG = Buffer.from(
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
  'base64'
);
const TINY_PNG_BASE64 = TINY_PNG.toString('base64');

interface PromptBlock {
  type: string;
  text?: string;
  data?: string;
  mimeType?: string;
}
interface PromptParams {
  prompt: PromptBlock[];
}

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: SESSION_ID,
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

/** Bracketed paste, the sequence a terminal sends for a dropped file path. */
const paste = (tc: AcpTestCase, text: string) =>
  tc.sendKeys(`\x1b[200~${text}\x1b[201~`);

const imageBlocks = (params: PromptParams) =>
  params.prompt.filter((block) => block.type === 'image');
const promptText = (params: PromptParams) =>
  params.prompt
    .filter((block) => block.type === 'text')
    .map((block) => block.text ?? '')
    .join('');

describe('a pasted local image path (wire)', () => {
  let tc: AcpTestCase | null = null;
  let directory: string;
  let imagePath: string;

  beforeAll(() => {
    directory = mkdtempSync(join(tmpdir(), 'kiro-image-attach-'));
    imagePath = join(directory, 'diagram.png');
    writeFileSync(imagePath, TINY_PNG);
  });

  afterAll(() => rmSync(directory, { recursive: true, force: true }));

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('shows a chip instead of the path and sends the bytes with the turn', async () => {
    tc = new AcpTestCase({ testName: 'image-attachment-send' });
    setupHandshake(tc);
    const prompts: PromptParams[] = [];
    tc.mock.on<PromptParams, { stopReason: string }>(
      'session/prompt',
      (params) => {
        prompts.push(params);
        return { stopReason: 'end_turn' };
      }
    );

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore((state) => state.isInitialized, 10_000);

    await paste(tc, imagePath);
    await tc.waitForVisibleText('diagram.png', 5000);
    // The chip replaces the path: the directory it came from is not on screen.
    expect(tc.getSnapshotFormatted()).not.toContain(directory);

    await tc.sendKeys(' what is this');
    await tc.pressEnter();
    await tc.waitForStore(() => prompts.length > 0, 10000);

    const images = imageBlocks(prompts[0]!);
    expect(images).toHaveLength(1);
    expect(images[0]).toMatchObject({
      mimeType: 'image/png',
      data: TINY_PNG_BASE64,
    });
    // The path rides along as text so the model can name the file it sees.
    expect(promptText(prompts[0]!)).toContain(imagePath);
  }, 30000);

  it('queues mid-turn rather than steering, then sends the bytes', async () => {
    tc = new AcpTestCase({ testName: 'image-attachment-mid-turn' });
    setupHandshake(tc);

    const prompts: PromptParams[] = [];
    let releaseFirstTurn!: () => void;
    const firstTurnHeld = new Promise<void>((resolve) => {
      releaseFirstTurn = resolve;
    });
    tc.mock.on<PromptParams, { stopReason: string }>(
      'session/prompt',
      async (params) => {
        prompts.push(params);
        // Hold the first turn open so the next submission lands mid-turn.
        if (prompts.length === 1) await firstTurnHeld;
        return { stopReason: 'end_turn' };
      }
    );
    let steered = 0;
    tc.mock.on('_session/steer', () => {
      steered += 1;
      return {};
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore((state) => state.isInitialized, 10_000);

    await tc.sendKeys('start working');
    await tc.pressEnter();
    await tc.waitForStore((state) => state.isProcessing, 10000);

    await paste(tc, imagePath);
    await tc.waitForVisibleText('diagram.png', 5000);
    await tc.pressEnter();

    // Steering carries text only, so an image-bearing prompt waits in the queue.
    const queued = await tc.waitForStore(
      (state) => state.queuedMessages.length === 1,
      5000
    );
    expect(queued.queuedMessages[0]).toContain(imagePath);
    expect(steered).toBe(0);

    releaseFirstTurn();
    await tc.waitForStore(() => prompts.length > 1, 15000);

    const images = imageBlocks(prompts[1]!);
    expect(images).toHaveLength(1);
    expect(images[0]).toMatchObject({
      mimeType: 'image/png',
      data: TINY_PNG_BASE64,
    });
  }, 40000);
});
