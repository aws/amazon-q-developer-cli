/**
 * E2E regression for unreadable diff colors on light terminals (P468472407).
 *
 * When the base theme is kiroSafe (terminal background unknown — SSH, failed
 * detection), lite-mode diffs must render git-style: named green/red
 * foregrounds (terminal-palette adaptive, readable on any background) and NO
 * background SGR. The original bug hardcoded dark bg tints on this path,
 * which rendered near-black default-fg text on dark blocks in light terminals.
 *
 * Exercises the full stack (ThemeProvider -> buildRenderTheme ->
 * renderUnifiedDiff) through both diff surfaces: the approval prompt and, after
 * approving, the scrollback tool-call rendering.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import stripAnsi from 'strip-ansi';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const GREEN_FG = '\x1b[32m';
const RED_FG = '\x1b[31m';

/**
 * True if the string sets any background color: basic (40-47, 100-107),
 * bg-reset (49), or extended (48;5;n / 48;2;r;g;b). Extended FOREGROUND
 * payloads (38;...) are skipped so e.g. `38;2;48;10;10` can't false-positive.
 */
function hasBgSgr(s: string): boolean {
  // eslint-disable-next-line no-control-regex
  const seqs = s.match(/\x1b\[[0-9;]*m/g) ?? [];
  for (const seq of seqs) {
    const params = seq
      .slice(2, -1)
      .split(';')
      .map((p) => (p === '' ? 0 : Number(p)));
    for (let i = 0; i < params.length; i++) {
      const p = params[i]!;
      if (p === 38) {
        i += params[i + 1] === 2 ? 4 : 2;
        continue;
      }
      if (p === 48 || p === 49) return true;
      if ((p >= 40 && p <= 47) || (p >= 100 && p <= 107)) return true;
    }
  }
  return false;
}

describe('hasBgSgr (guard self-check)', () => {
  it.each([
    ['pre-fix truecolor tint', '\x1b[48;2;31;45;34mtext\x1b[0m', true],
    ['256-color tint', '\x1b[48;5;236mtext\x1b[0m', true],
    ['basic bg', '\x1b[41mtext\x1b[0m', true],
    ['bright bg', '\x1b[102mtext\x1b[0m', true],
    ['named green fg only', '\x1b[32mtext\x1b[39m', false],
    ['extended fg with 48 payload', '\x1b[38;2;48;10;10mtext\x1b[39m', false],
    ['bold + dim only', '\x1b[1m\x1b[2mtext\x1b[0m', false],
  ] as const)('%s -> %p', (_name, input, expected) => {
    expect(hasBgSgr(input)).toBe(expected);
  });
});

describe('lite diff on kiroSafe theme (unknown terminal background)', () => {
  let testCase: E2ETestCase | null = null;
  let tempDir = '';

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
    if (tempDir) {
      try {
        fs.rmSync(tempDir, { recursive: true });
      } catch {
        /* ignore */
      }
      tempDir = '';
    }
  });

  it('renders diff rows with named green/red foregrounds and no bg SGR', async () => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-kirosafe-'));
    const filePath = path.join(tempDir, 'notes.md');
    // Prose (markdown) mirrors the reported worst case: no syntax tokens, so
    // without the fix the row text had no color of its own at all. Unique
    // markers keep raw-output line lookups unambiguous.
    const oldLine = 'The quick REMOVEDMARKER line of prose.';
    const newLine = 'The quick ADDEDMARKER line of prose.';
    fs.writeFileSync(filePath, oldLine + '\n');

    testCase = await E2ETestCase.builder()
      .withTestName('lite-diff-kirosafe')
      .withTerminal({ width: 100, height: 30 })
      .withLite()
      .withEnv({ KIRO_TERMINAL_THEME: 'safe' })
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'tool-kirosafe-1',
            name: 'write',
            input: JSON.stringify({
              command: 'strReplace',
              path: filePath,
              oldStr: oldLine,
              newStr: newLine,
            }),
            stop: true,
          },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);
    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: { kind: 'AssistantResponseEvent', data: { content: 'Done.' } },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    await testCase.sendKeys('update the note');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Surface 1: approval prompt renders the diff.
    await testCase.waitForText('needs approval', 15000);
    assertDiffRowInvariants(testCase.getOutput());

    // Surface 2: approve, then the scrollback tool-call rendering repeats the
    // diff through LiteLayout's separate call site.
    await testCase.sendKeys('y');
    await testCase.waitForText('added 1 line', 15000);
    assertDiffRowInvariants(testCase.getOutput());
  }, 45000);

  function assertDiffRowInvariants(rawOutput: string): void {
    const rawLines = rawOutput.split(/\r?\n/);
    const addedLines = rawLines.filter((l) =>
      stripAnsi(l).includes('ADDEDMARKER')
    );
    const removedLines = rawLines.filter((l) =>
      stripAnsi(l).includes('REMOVEDMARKER')
    );
    expect(addedLines.length).toBeGreaterThan(0);
    expect(removedLines.length).toBeGreaterThan(0);

    for (const line of [...addedLines, ...removedLines]) {
      expect(hasBgSgr(line)).toBe(false);
    }
    // Named colors keep the rows readable on any terminal background — this
    // is the "actually readable" guarantee, not just "no dark tint".
    for (const line of addedLines) {
      expect(line).toContain(GREEN_FG);
    }
    for (const line of removedLines) {
      expect(line).toContain(RED_FG);
    }
  }
});
