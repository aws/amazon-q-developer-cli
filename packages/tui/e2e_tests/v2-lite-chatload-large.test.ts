/**
 * Field-shape repro: in-session `/chat` picker load (v2 engine, lite) of a
 * LARGE on-disk session (>70 messages, wrapped lines, distinctive last turn),
 * then immediately type new input. Field report: the new input renders at the
 * old prompt's position ("replaces the old user input") until the response
 * arrives.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { E2ETestCase } from './E2ETestCase';

const SESSION_ID = randomUUID();
const TURNS = 35;

function v2SessionJson(cwd: string): string {
  // The session listing deserializes into a typed struct and silently skips
  // entries that fail to parse — `session_state` and friends are required.
  // Clone a real captured session and patch only the identity fields.
  const fixture = JSON.parse(
    readFileSync(
      resolve(
        __dirname,
        '../../../crates/chat-cli-v2/src/agent/kas/fixtures/v2/basic_fs_tools/session.json'
      ),
      'utf8'
    )
  );
  fixture.session_id = SESSION_ID;
  fixture.cwd = cwd;
  fixture.title = 'Large investigation session';
  fixture.updated_at = new Date().toISOString();
  return JSON.stringify(fixture);
}

function v2MessagesJsonl(): string {
  const lines: string[] = [];
  const prompt = (text: string) =>
    JSON.stringify({
      version: 'v1',
      kind: 'Prompt',
      data: {
        message_id: randomUUID(),
        content: [{ kind: 'text', data: text }],
      },
    });
  const assistant = (text: string) =>
    JSON.stringify({
      version: 'v1',
      kind: 'AssistantMessage',
      data: {
        message_id: randomUUID(),
        content: [{ kind: 'text', data: text }],
      },
    });
  for (let t = 0; t < TURNS; t++) {
    lines.push(prompt(`EARLYQ-${t} question about the service internals`));
    lines.push(
      assistant(
        `EARLY-${t} reply body with wrapped filler content that greatly exceeds the terminal width for physical row accounting purposes`
      )
    );
  }
  lines.push(prompt('OLDQUESTION why ninety seconds exactly'));
  lines.push(
    assistant(
      Array.from(
        { length: 10 },
        (_, i) =>
          `HISTPARA-${i} detailed analysis paragraph with wrapped filler content exceeding the terminal width`
      ).join('\n\n')
    )
  );
  return lines.join('\n') + '\n';
}

describe('v2 lite: in-session /chat load of a large session, then type', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('typed input renders as a new row below the replayed history', async () => {
    // The session's cwd must match the TUI's cwd for the picker listing;
    // the harness spawns with process.cwd() by default.
    const cwd = process.cwd();
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 80, height: 14 })
      .withTestName('v2-lite-chatload-large')
      .withLite()
      .withGlobalSettings({ 'chat.preserveScrollback': true })
      .withPrelaunchFile(`${SESSION_ID}.json`, v2SessionJson(cwd))
      .withPrelaunchFile(`${SESSION_ID}.jsonl`, v2MessagesJsonl())
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    // Open the /chat picker and select the seeded session.
    for (const c of '/chat') {
      await testCase.sendKeys(c);
      await testCase.sleepMs(30);
    }
    await testCase.pressEnter();
    await testCase.waitForText('Large investigation session', 10000);
    await testCase.pressEnter();

    // Wait for the replay to visibly complete (tail of the last turn).
    await testCase.waitForText('HISTPARA-9', 20000);
    await testCase.sleepMs(700);

    // Hold the response back to keep the pre-response window open.
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'NEWRESPONSE ack' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    for (const c of 'BRANDNEWINPUT sup') {
      await testCase.sendKeys(c);
    }
    await testCase.pressEnter();
    await testCase.sleepMs(700);

    // Pre-response window: history must remain unique and entirely above the new input.
    const during = testCase.getSnapshot();
    const dCount = (n: string) => during.filter((l) => l.includes(n)).length;
    const dLastIdx = (n: string) => during.findLastIndex((l) => l.includes(n));
    expect(dCount('You: BRANDNEWINPUT')).toBe(1);
    expect(dCount('You: OLDQUESTION')).toBe(1);
    for (let i = 0; i < 10; i++) {
      expect(dCount(`HISTPARA-${i}`)).toBe(1);
    }
    expect(dLastIdx('You: BRANDNEWINPUT')).toBeGreaterThan(
      dLastIdx('HISTPARA-9')
    );

    await testCase.waitForText('NEWRESPONSE', 15000);
    await testCase.sleepMs(700);

    const all = testCase.getSnapshot();
    const count = (n: string) => all.filter((l) => l.includes(n)).length;
    const idxLast = (n: string) =>
      all.reduce((acc, l, i) => (l.includes(n) ? i : acc), -1);

    // The old prompt and replayed history survive once, above the new input.
    expect(count('You: OLDQUESTION')).toBe(1);
    expect(count('You: BRANDNEWINPUT')).toBe(1);
    for (let i = 0; i < 10; i++) {
      expect(count(`HISTPARA-${i}`)).toBe(1);
    }
    expect(idxLast('You: BRANDNEWINPUT')).toBeGreaterThan(
      idxLast('HISTPARA-9')
    );
  }, 120000);
});
