import './setup-chalk-level.js';

import {
  describe,
  test,
  it,
  expect,
  beforeEach,
  beforeAll,
  afterAll,
} from 'vitest';
import chalk from 'chalk';
import { mkdtempSync, rmSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import {
  renderToolCall,
  renderMessageToText,
  formatToolArgLines,
  formatTaskToolBody,
} from '../render.js';
import {
  setVerboseConfig,
  resetVerboseCache,
  DEFAULT_DISPLAY,
  type VerboseDisplayConfig,
} from '../verbose.js';
import stripAnsi from 'strip-ansi';

// Redirect KIRO_HOME so the verbose tests don't stomp on the developer's
// real ~/.kiro/settings/lite_verbose.json. The directory is removed
// after the suite finishes.
let tmpHome: string | undefined;
let originalKiroHome: string | undefined;
beforeAll(() => {
  originalKiroHome = process.env.KIRO_HOME;
  tmpHome = mkdtempSync(join(tmpdir(), 'kiro-verbose-test-'));
  process.env.KIRO_HOME = tmpHome;
});
afterAll(() => {
  if (originalKiroHome === undefined) {
    delete process.env.KIRO_HOME;
  } else {
    process.env.KIRO_HOME = originalKiroHome;
  }
  if (tmpHome) {
    try {
      rmSync(tmpHome, { recursive: true, force: true });
    } catch {
      // best-effort cleanup
    }
  }
});

// Restore the all-flags-on / default-cap display so a suite that mutated the
// global verbose config doesn't leak into the next file's expectations.
function restoreFullDefaults() {
  setVerboseConfig({
    filters: [],
    display: {
      showToolReasoning: true,
      toolArgsMode: 'block',
      showElapsed: true,
      subagent: {
        pipeline: true,
        prompts: true,
        roles: true,
        deps: true,
        responses: true,
      },
      showThinkingContent: true,
      showTasks: true,
      argsMaxLines: null,
      outputMaxLines: null,
      argsMaxChars: 80,
      outputMaxChars: null,
    },
  });
  resetVerboseCache();
}

describe('renderToolCall', () => {
  // STATUS-SLOT CONTRACT: the trailing status glyph reflects the truthful
  // in-flight state. A threaded runningSpinner replaces ' ...' on non-trivial
  // tools (so the row shows motion while args/diff/reasoning already match the
  // settled appearance), but trivial tools (read/grep/glob) ignore it — too
  // short-lived for a spinner to inform. awaitingApproval always wins over the
  // spinner with a yellow ' ...' (`\x1b[33m`, matching the approval prompt's
  // [t] hotkey) since the agent isn't progressing while approval is pending.
  test.each([
    [
      'running shows ellipsis',
      { name: 'execute_bash', status: 'running' as const },
      ['execute_bash', '...'],
      [],
      [],
    ],
    [
      'running non-trivial + spinner: glyph replaces ellipsis',
      { name: 'execute_bash', status: 'running' as const, runningSpinner: '⠋' },
      ['execute_bash', '⠋'],
      ['...'],
      [],
    ],
    [
      'running trivial + spinner: keeps ellipsis, ignores glyph',
      {
        name: 'fs_read',
        status: 'running' as const,
        runningSpinner: '⠋',
        isTrivial: true,
      },
      ['fs_read', '...'],
      ['⠋'],
      [],
    ],
    [
      'awaitingApproval non-trivial: yellow ellipsis beats spinner',
      {
        name: 'execute_bash',
        status: 'running' as const,
        runningSpinner: '⠋',
        awaitingApproval: true,
      },
      ['execute_bash', '...'],
      ['⠋'],
      ['\x1b[33m'],
    ],
    [
      'awaitingApproval fires on trivial tools too',
      {
        name: 'fs_read',
        status: 'running' as const,
        isTrivial: true,
        awaitingApproval: true,
      },
      ['fs_read', '...'],
      [],
      ['\x1b[33m'],
    ],
    [
      'done with elapsed shows time',
      { name: 'fs_write', status: 'done' as const, elapsed: 1500 },
      ['fs_write', '1.5s'],
      [],
      [],
    ],
    [
      'done without elapsed shows nothing extra',
      { name: 'fs_write', status: 'done' as const },
      ['fs_write'],
      [],
      [],
    ],
    [
      'error shows FAILED',
      { name: 'shell', status: 'error' as const },
      ['FAILED'],
      [],
      [],
    ],
  ])('%s', (_name, input, contains, notContains, rawContains) => {
    const result = renderToolCall(input);
    const plain = stripAnsi(result);
    for (const c of contains) expect(plain).toContain(c);
    for (const n of notContains) expect(plain).not.toContain(n);
    for (const r of rawContains) expect(result).toContain(r);
  });

  test('shows MCP server source', () => {
    const result = renderToolCall({
      name: 'InternalSearch',
      mcpServer: 'builder-mcp',
      status: 'done',
    });
    expect(result).toContain('builder-mcp');
    expect(result).toContain('InternalSearch');
  });

  test('shows description when provided', () => {
    const result = renderToolCall({
      name: 'execute_bash',
      description: 'Running tests',
      status: 'running',
    });
    expect(result).toContain('Running tests');
  });

  test('trivial tools are dimmed', () => {
    const result = renderToolCall({
      name: 'fs_read',
      status: 'done',
      isTrivial: true,
    });
    expect(result).toContain('fs_read');
  });

  test('inline arg chip renders next to tool name', () => {
    const result = renderToolCall({
      name: 'shell',
      inlineArg: '[git status]',
      status: 'done',
    });
    // Strip ANSI to assert structural shape on a single line.
    const plain = stripAnsi(result);
    expect(plain).toContain('shell [git status]');
    // No newline → reasoning isn't taking up a line below.
    expect(plain.split('\n').length).toBe(1);
  });

  test('inline arg + reasoning: args inline, reasoning on its own line below', () => {
    const result = renderToolCall({
      name: 'shell',
      inlineArg: '[git status]',
      description: 'Check the working tree state',
      status: 'done',
    });
    const lines = stripAnsi(result).split('\n');
    expect(lines.length).toBe(2);
    expect(lines[0]).toContain('shell [git status]');
    // Reasoning on its own indented line below — never inline next to the
    // tool name when an inline arg is present.
    expect(lines[0]).not.toContain('Check the working tree state');
    expect(lines[1]).toContain('Check the working tree state');
  });

  test('reasoning without inline arg keeps legacy inline-on-first-line shape', () => {
    const result = renderToolCall({
      name: 'shell',
      description: 'Check the working tree state',
      status: 'done',
    });
    const lines = stripAnsi(result).split('\n');
    expect(lines.length).toBe(1);
    expect(lines[0]).toContain('shell');
    expect(lines[0]).toContain('Check the working tree state');
  });

  test('multi-line reasoning + inline arg: every reasoning line indents below', () => {
    const result = renderToolCall({
      name: 'shell',
      inlineArg: '[git status]',
      description: 'First reason\nsecond reason',
      status: 'done',
    });
    const lines = stripAnsi(result).split('\n');
    expect(lines.length).toBe(3);
    expect(lines[0]).toContain('shell [git status]');
    expect(lines[0]).not.toContain('First reason');
    expect(lines[1]).toContain('First reason');
    expect(lines[2]).toContain('second reason');
  });
});

describe('formatToolArgLines wrap behavior', () => {
  // Long string values used to wrap back to column 0 because we emitted the
  // value as a single chalk.dim'd line and let the terminal soft-wrap. Now
  // wrapping happens at the renderer with the parent's indent applied to
  // every continuation line, so the visual nesting is preserved.
  test('long string value wraps with continuation indent matching the value column', () => {
    const longUrl =
      'https://very-long-domain-name.example.com/path/to/some/deeply/nested/resource?query=foo&other=bar&baz=quux';
    const content = JSON.stringify({ url: longUrl });
    const lines = formatToolArgLines('fetch', content, 40);
    expect(lines).not.toBeNull();
    const stripped = (lines ?? []).map(stripAnsi);
    // First line: "  url: <head>"
    expect(stripped[0]).toMatch(/^ {2}url: /);
    // At least one continuation
    expect(stripped.length).toBeGreaterThan(1);
    // All continuation lines start at column 4 (one level deeper than the
    // top-level "  url:" key, mirroring how an object's children would indent).
    for (const line of stripped.slice(1)) {
      expect(line.startsWith('    ')).toBe(true);
      // Line stays within the requested width
      expect(line.length).toBeLessThanOrEqual(40);
    }
  });

  test('nested object: long inner string wraps at deeper indent', () => {
    const longText = 'a'.repeat(80) + ' ' + 'b'.repeat(40);
    const content = JSON.stringify({
      messages: [{ role: 'USER', content: longText }],
    });
    const lines = formatToolArgLines('remember', content, 40);
    expect(lines).not.toBeNull();
    const stripped = (lines ?? []).map(stripAnsi);
    // The "content:" key sits at depth 3 (messages → - → content), so its
    // continuation should indent 4 levels = 8 spaces.
    const contentIdx = stripped.findIndex((l) => /content: a/.test(l));
    expect(contentIdx).toBeGreaterThanOrEqual(0);
    // Continuation indent matches: every line after `content:` until next key
    // starts with at least 8 spaces (level 4 indent).
    for (let i = contentIdx + 1; i < stripped.length; i++) {
      const line = stripped[i]!;
      // Stop at the next sibling key (lines that aren't pure indented text)
      if (!line.startsWith('    ')) break;
      expect(line.startsWith('        ')).toBe(true);
    }
  });

  test('short value with no newline returns a single line', () => {
    const content = JSON.stringify({ key: 'short' });
    const lines = formatToolArgLines('whatever', content, 80);
    expect(lines).not.toBeNull();
    expect(lines).toHaveLength(1);
    expect(stripAnsi(lines![0]!)).toBe('  key: short');
  });

  // wrapAtWords used to be O(n²) in the line length: an outer
  // `while (visibleWidth(remaining) > width)` recomputed width over the full
  // remaining tail every iteration, so a 100K-char unbreakable run froze the
  // renderer for 30+s. Single-pass refactor pulls this under 50ms.
  test('100K-char unbreakable value wraps in well under a second', () => {
    const huge = 'x'.repeat(100_000);
    const content = JSON.stringify({ blob: huge });
    const t0 = Date.now();
    const lines = formatToolArgLines('paste', content, 80);
    const elapsed = Date.now() - t0;
    expect(lines).not.toBeNull();
    expect(lines!.length).toBeGreaterThan(1000);
    // Generous ceiling — typical run is <30ms; 500ms catches any regression
    // back toward the prior O(n²) shape (which was 30+s on the same input).
    expect(elapsed).toBeLessThan(500);
  });

  // visibleWidth measures combining marks correctly (Mn class is zero-width
  // via twinki); the wrap path's per-character width math must agree, so
  // decomposed and precomposed forms wrap to the same number of rows.
  test('decomposed combining marks (café = e + U+0301) wrap identically to precomposed', () => {
    const decomposed = 'café '.repeat(40);
    const precomposed = 'café '.repeat(40);
    const decompLines = formatToolArgLines(
      'paste',
      JSON.stringify({ s: decomposed }),
      40
    );
    const precompLines = formatToolArgLines(
      'paste',
      JSON.stringify({ s: precomposed }),
      40
    );
    expect(decompLines).not.toBeNull();
    expect(precompLines).not.toBeNull();
    expect(decompLines!.length).toBe(precompLines!.length);
  });

  // Multi-line string args used to collapse to 1 line + "(50 lines)" total-
  // count marker when argsMaxChars was set (default 120). That conflated
  // "value char cap" with "line collapse" — argsMaxChars should ONLY clip
  // per-line; line count is a separate concern. After the fix, multi-line
  // values always render up to 5 lines with a delta-count marker mirroring
  // the output bar's "+N more lines above" idiom.
  test('multi-line string with argsMaxChars set: renders up to 5 lines, not 1', () => {
    const sevenLines = Array.from({ length: 7 }, (_, i) => `line${i}`).join(
      '\n'
    );
    const content = JSON.stringify({ command: sevenLines });
    const lines = formatToolArgLines('shell', content, 120, 120);
    expect(lines).not.toBeNull();
    const stripped = (lines ?? []).map(stripAnsi);
    // 5 visible source lines (line0..line4) — line0 sits on the head row
    // with the key, line1..line4 on continuation rows below.
    expect(stripped.some((l) => /command:\s*line0/.test(l))).toBe(true);
    expect(stripped.some((l) => l.includes('line1'))).toBe(true);
    expect(stripped.some((l) => l.includes('line4'))).toBe(true);
    // Hidden lines collapse behind the marker.
    expect(stripped.some((l) => l.includes('line5'))).toBe(false);
    expect(stripped.some((l) => l.includes('line6'))).toBe(false);
  });

  test('multi-line marker uses delta count (+N more lines), not total', () => {
    const fifty = Array.from({ length: 50 }, (_, i) => `line${i}`).join('\n');
    const content = JSON.stringify({ command: fifty });
    const lines = formatToolArgLines('shell', content, 120, 120);
    expect(lines).not.toBeNull();
    const joined = (lines ?? []).map(stripAnsi).join('\n');
    // Delta = 50 - 5 visible = 45 hidden. Matches the output bar's
    // "+N more lines above" idiom so the two sections read consistently.
    expect(joined).toMatch(/\.\.\. \(\+45 more lines\)/);
    // The pre-fix total-count form ("(50 lines)") MUST NOT appear — that
    // was the marker that confused users into thinking 50 lines were
    // hidden when only 45 actually were.
    expect(joined).not.toMatch(/\(50 lines\)/);
  });

  test('multi-line marker omitted when value fits in 5 lines', () => {
    const four = ['a', 'b', 'c', 'd'].join('\n');
    const content = JSON.stringify({ command: four });
    const lines = formatToolArgLines('shell', content, 120, 120);
    expect(lines).not.toBeNull();
    const joined = (lines ?? []).map(stripAnsi).join('\n');
    expect(joined).not.toMatch(/more lines/);
    // All 4 source lines visible.
    expect(joined).toContain('a');
    expect(joined).toContain('d');
  });

  test('argsMaxChars clips EACH line of a multi-line value, not just the head', () => {
    // Per-line clip is what the knob's name says. Without this, only the
    // first line was clipped (when collapse fired) and continuation rows
    // rendered uncapped — confusing inconsistency.
    const content = JSON.stringify({
      command: 'aaaaaaaaaaaaaaaa\nbbbbbbbbbbbbbbbb\ncccccccccccccccc',
    });
    // maxChars=8 → each line clips to 7 chars + "…".
    const lines = formatToolArgLines('shell', content, 120, 8);
    expect(lines).not.toBeNull();
    const stripped = (lines ?? []).map(stripAnsi);
    // First line attached to the key, clipped.
    expect(stripped[0]).toMatch(/command: aaaaaaa…/);
    // Continuation rows also clipped — same per-line cap applied.
    expect(stripped.some((l) => /^\s+bbbbbbb…/.test(l))).toBe(true);
    expect(stripped.some((l) => /^\s+ccccccc…/.test(l))).toBe(true);
  });

  // P438130055: the user-facing "unlimited" option for argsMaxLines saves
  // `null`, but a hardcoded MULTI_LINE_VISIBLE = 5 inside formatArgLines
  // still clipped each multi-line string value at 5 lines + a delta marker.
  // The unlimited toggle now propagates through perValueLineCap so a 50-line
  // shell heredoc renders all 50 lines with no marker.
  test('multi-line value renders all lines when perValueLineCap is null', () => {
    const fifty = Array.from({ length: 50 }, (_, i) => `line${i}`).join('\n');
    const content = JSON.stringify({ command: fifty });
    const lines = formatToolArgLines('shell', content, 120, null, null);
    expect(lines).not.toBeNull();
    const joined = (lines ?? []).map(stripAnsi).join('\n');
    expect(joined).not.toMatch(/more lines/);
    // First, middle, and last source lines all visible.
    expect(joined).toContain('line0');
    expect(joined).toContain('line25');
    expect(joined).toContain('line49');
  });

  test('formatToolArgLines defaults to a 5-line per-value cap (back-compat)', () => {
    // Locks the default. Callers that don't pass perValueLineCap (e.g.
    // ApprovalPrompt, formatJsonAsBarLines for output rendering) keep the
    // historical 5-line clamp so the unlimited fix doesn't accidentally
    // unleash unbounded multi-line rendering everywhere.
    const ten = Array.from({ length: 10 }, (_, i) => `line${i}`).join('\n');
    const content = JSON.stringify({ command: ten });
    const lines = formatToolArgLines('shell', content, 120);
    expect(lines).not.toBeNull();
    const joined = (lines ?? []).map(stripAnsi).join('\n');
    // 10 source lines - 5 visible = 5 hidden.
    expect(joined).toMatch(/\.\.\. \(\+5 more lines\)/);
    expect(joined).toContain('line0');
    expect(joined).toContain('line4');
    expect(joined).not.toContain('line5');
  });

  test('explicit perValueLineCap of 3 clips a 50-line value to 3 + marker', () => {
    // Locks that the cap is an actual visible-line count. If a future
    // refactor changes the param to "max hidden lines" or similar, this
    // breaks loudly.
    const fifty = Array.from({ length: 50 }, (_, i) => `line${i}`).join('\n');
    const content = JSON.stringify({ command: fifty });
    const lines = formatToolArgLines('shell', content, 120, null, 3);
    expect(lines).not.toBeNull();
    const joined = (lines ?? []).map(stripAnsi).join('\n');
    expect(joined).toContain('line0');
    expect(joined).toContain('line2');
    expect(joined).not.toContain('line3');
    expect(joined).toMatch(/\.\.\. \(\+47 more lines\)/);
  });
});

describe('formatTaskToolBody', () => {
  test('create command renders numbered task list under a description block', () => {
    const content = JSON.stringify({
      command: 'create',
      task_list_description: 'Add lite rendering for tasks',
      tasks: [
        { task_description: 'Investigate the schema' },
        { task_description: 'Implement the renderer' },
        { task_description: 'Add tests' },
      ],
    });
    const result = formatTaskToolBody(content, 100);
    expect(result).not.toBeNull();
    expect(result!.command).toBe('create');
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    expect(text).toContain('description:');
    expect(text).toContain('Add lite rendering for tasks');
    expect(text).toContain('1. Investigate the schema');
    expect(text).toContain('2. Implement the renderer');
    expect(text).toContain('3. Add tests');
    // Tree connectors: ├─ for non-last rows, └─ for the last.
    expect(text).toContain('├─');
    expect(text).toContain('└─');
    // Single-task `create` would only use └─ — verify that with a separate
    // call below; here we want both.
  });

  test('create with a single task uses only the corner connector', () => {
    const content = JSON.stringify({
      command: 'create',
      task_list_description: 'just one thing',
      tasks: [{ task_description: 'Only task' }],
    });
    const result = formatTaskToolBody(content, 100);
    expect(result).not.toBeNull();
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    expect(text).toContain('└─');
    expect(text).not.toContain('├─');
  });

  test('create renders optional details under the subject in dim style', () => {
    const content = JSON.stringify({
      command: 'create',
      task_list_description: 'list',
      tasks: [
        {
          task_description: 'First task',
          details: 'Some additional context about how to do it',
        },
      ],
    });
    const result = formatTaskToolBody(content, 100);
    expect(result).not.toBeNull();
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    expect(text).toContain('First task');
    expect(text).toContain('Some additional context about how to do it');
    // Details line indented under the subject (past the `└─ N. ` prefix).
    const detailsLine = result!.bodyLines.find((l) =>
      stripAnsi(l).includes('Some additional context')
    );
    expect(detailsLine).toBeDefined();
    expect(stripAnsi(detailsLine!).startsWith('       ')).toBe(true);
  });

  test('long task subjects wrap at termCols with continuation indent', () => {
    const content = JSON.stringify({
      command: 'create',
      task_list_description: 'd',
      tasks: [
        {
          task_description:
            'this is a very long task subject that should wrap across multiple lines when the terminal width is small',
        },
      ],
    });
    const result = formatTaskToolBody(content, 50);
    expect(result).not.toBeNull();
    const stripped = result!.bodyLines.map(stripAnsi);
    // Every line stays within ~50 cols (allow a couple extra for ANSI/whitespace edge).
    for (const line of stripped) {
      expect(line.length).toBeLessThanOrEqual(54);
    }
    // Continuation rows align under the subject (past `  └─ N. `).
    const head = stripped.findIndex((l) => l.includes('this is a very long'));
    expect(head).toBeGreaterThanOrEqual(0);
    if (head + 1 < stripped.length) {
      const cont = stripped[head + 1]!;
      // Should not crash to col 0 — at minimum has the prefix-width pad.
      expect(cont.startsWith('       ')).toBe(true);
    }
  });

  test('add command renders new_tasks with optional new_description', () => {
    const content = JSON.stringify({
      command: 'add',
      new_description: 'expanded scope',
      new_tasks: [{ task_description: 'New thing' }],
    });
    const result = formatTaskToolBody(content, 100);
    expect(result).not.toBeNull();
    expect(result!.command).toBe('add');
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    expect(text).toContain('description:');
    expect(text).toContain('expanded scope');
    expect(text).toContain('1. New thing');
  });

  test('complete command renders ID chips, notes block, and modified files', () => {
    const content = JSON.stringify({
      command: 'complete',
      completed_task_ids: ['1', '2', '3'],
      context_update:
        'Found that the schema lives in the agent crate and is stable across versions.',
      modified_files: [
        'packages/tui/src/lite/render.ts',
        'packages/tui/src/lite/__tests__/render.test.ts',
      ],
    });
    const result = formatTaskToolBody(content, 120);
    expect(result).not.toBeNull();
    expect(result!.command).toBe('complete');
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    expect(text).toContain('completed:');
    expect(text).toContain('#1');
    expect(text).toContain('#2');
    expect(text).toContain('#3');
    expect(text).toContain('notes:');
    expect(text).toContain('schema lives in the agent crate');
    expect(text).toContain('files:');
    expect(text).toContain('- packages/tui/src/lite/render.ts');
  });

  test('complete without notes or files renders only the ID chips', () => {
    const content = JSON.stringify({
      command: 'complete',
      completed_task_ids: ['7'],
      context_update: '   ', // whitespace-only — should be treated as empty
    });
    const result = formatTaskToolBody(content, 80);
    expect(result).not.toBeNull();
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    expect(text).toContain('completed:');
    expect(text).toContain('#7');
    expect(text).not.toContain('notes:');
    expect(text).not.toContain('files:');
  });

  test('remove command shows IDs and optional new description', () => {
    const content = JSON.stringify({
      command: 'remove',
      remove_task_ids: ['2', '5'],
      new_description: 'narrowed plan',
    });
    const result = formatTaskToolBody(content, 100);
    expect(result).not.toBeNull();
    expect(result!.command).toBe('remove');
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    expect(text).toContain('removed:');
    expect(text).toContain('#2');
    expect(text).toContain('#5');
    expect(text).toContain('description:');
    expect(text).toContain('narrowed plan');
  });

  test('list command returns empty body — header line is enough', () => {
    const result = formatTaskToolBody(JSON.stringify({ command: 'list' }), 80);
    expect(result).not.toBeNull();
    expect(result!.command).toBe('list');
    expect(result!.bodyLines).toHaveLength(0);
  });

  test('returns null on invalid JSON, empty, or unknown command', () => {
    expect(formatTaskToolBody('')).toBeNull();
    expect(formatTaskToolBody('not-json')).toBeNull();
    expect(formatTaskToolBody(JSON.stringify({}))).toBeNull();
    expect(formatTaskToolBody(JSON.stringify({ command: 'bogus' }))).toBeNull();
  });

  test('skips tasks with empty or missing task_description', () => {
    // Defensive: a malformed args payload (rare, but possible if the
    // backend ever ships partial JSON during streaming) shouldn't render
    // a phantom row with just a number and no text.
    const content = JSON.stringify({
      command: 'create',
      task_list_description: 'd',
      tasks: [
        { task_description: 'real one' },
        { task_description: '' },
        { task_description: '   ' },
        {} as { task_description?: string },
      ],
    });
    const result = formatTaskToolBody(content, 80);
    expect(result).not.toBeNull();
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    expect(text).toContain('real one');
    // Only one task rendered — no 2./3./4. rows.
    expect(text).not.toMatch(/\b2\./);
    expect(text).not.toMatch(/\b3\./);
    expect(text).not.toMatch(/\b4\./);
  });
});

// Verbose-mode rendering. Toggling state via setVerboseConfig touches the
// real fs path, but the test only writes inside ~/.kiro and we reset back
// to defaults in afterAll so other test files don't see lingering state.
describe('verbose tool output rendering', () => {
  beforeEach(() => {
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
  });

  afterAll(() => {
    setVerboseConfig({ filters: [] });
    resetVerboseCache();
  });

  const toolMsg = (overrides: Record<string, unknown> = {}) => ({
    id: 't-verbose-1',
    role: 'tool_use' as const,
    name: 'execute_bash',
    content: JSON.stringify({ command: 'echo hi' }),
    isFinished: true,
    result: { status: 'success', output: 'hi from stdout\nsecond line' },
    ...overrides,
  });

  test('verbose off: no output bar even with result present', () => {
    setVerboseConfig({ filters: [] });
    const out = stripAnsi(renderMessageToText(toolMsg(), 'kiro_default'));
    expect(out).not.toContain('hi from stdout');
    // Sanity: tool name still renders.
    expect(out).toContain('execute_bash');
  });

  test('verbose on + filter "all": output bar renders with full text', () => {
    setVerboseConfig({ filters: ['all'] });
    const out = stripAnsi(renderMessageToText(toolMsg(), 'kiro_default'));
    expect(out).toContain('hi from stdout');
    expect(out).toContain('second line');
    // Bar prefix glyph shows up on each output row.
    expect(out).toContain('│');
  });

  test('verbose on but filter excludes this tool: no output bar', () => {
    setVerboseConfig({ filters: ['mcp'] });
    const out = stripAnsi(renderMessageToText(toolMsg(), 'kiro_default'));
    expect(out).not.toContain('hi from stdout');
  });

  test('filter "shell" lets bash through but not fs_read', () => {
    setVerboseConfig({ filters: ['shell'] });
    const bash = stripAnsi(
      renderMessageToText(toolMsg({ name: 'execute_bash' }), 'kiro_default')
    );
    expect(bash).toContain('hi from stdout');

    const read = stripAnsi(
      renderMessageToText(
        toolMsg({
          id: 't-verbose-2',
          name: 'fs_read',
          content: JSON.stringify({
            operations: [{ path: '/tmp/x' }],
          }),
          result: { status: 'success', output: 'file contents here' },
        }),
        'kiro_default'
      )
    );
    expect(read).not.toContain('file contents here');
  });

  test('filter targets a single MCP tool by exact name', () => {
    setVerboseConfig({
      filters: ['mcp__nova-memory-mcp__recall'],
    });
    const recall = stripAnsi(
      renderMessageToText(
        toolMsg({
          id: 't-verbose-3',
          name: 'mcp__nova-memory-mcp__recall',
          content: JSON.stringify({ query: 'history' }),
          result: { status: 'success', output: 'memory blob' },
        }),
        'kiro_default'
      )
    );
    expect(recall).toContain('memory blob');

    const remember = stripAnsi(
      renderMessageToText(
        toolMsg({
          id: 't-verbose-4',
          name: 'mcp__nova-memory-mcp__remember',
          content: JSON.stringify({ messages: [] }),
          result: { status: 'success', output: 'persisted' },
        }),
        'kiro_default'
      )
    );
    expect(remember).not.toContain('persisted');
  });

  test('error result renders red bar regardless of result.output', () => {
    setVerboseConfig({ filters: ['all'] });
    const out = stripAnsi(
      renderMessageToText(
        toolMsg({
          id: 't-verbose-5',
          result: { status: 'error', error: 'permission denied' },
        }),
        'kiro_default'
      )
    );
    expect(out).toContain('permission denied');
  });

  test('env var KIRO_LITE_VERBOSE=1 is a no-op when a saved config exists', () => {
    // Env var is now only a startup hint for first-time users — saved
    // configs win. With filters:[] persisted, the env var must NOT force
    // output back on.
    setVerboseConfig({ filters: [] });
    process.env.KIRO_LITE_VERBOSE = '1';
    resetVerboseCache();
    const out = stripAnsi(renderMessageToText(toolMsg(), 'kiro_default'));
    expect(out).not.toContain('hi from stdout');
  });

  // The dim "output:" header above the | bar disambiguates the output
  // section from the args block above it. Without it, the args block's
  // tail and the bar's first row read as one continuous chunk — especially
  // when the args section ends with its own "(+N more lines)" marker, which
  // visually rhymes with the bar's "+N more lines above" marker.
  test('output: header renders above the | bar on success', () => {
    setVerboseConfig({ filters: ['all'] });
    const out = stripAnsi(renderMessageToText(toolMsg(), 'kiro_default'));
    expect(out).toContain('output:');
    // Header lands ABOVE the first bar row, not below it.
    const headerIdx = out.indexOf('output:');
    const firstBarIdx = out.indexOf('│');
    expect(headerIdx).toBeGreaterThan(-1);
    expect(firstBarIdx).toBeGreaterThan(headerIdx);
  });

  test('output: header renders above red error bar on failure', () => {
    // Errors take a separate code path inside renderVerboseOutput; the
    // header should appear there too so the visual structure stays
    // consistent regardless of result.status.
    setVerboseConfig({ filters: ['all'] });
    const errMsg = toolMsg({
      id: 't-verbose-err',
      result: { status: 'error', error: 'command failed: exit 1' },
    });
    const out = stripAnsi(renderMessageToText(errMsg, 'kiro_default'));
    expect(out).toContain('output:');
    expect(out).toContain('command failed');
  });

  test('output: header is suppressed when no output renders', () => {
    // Empty/null output paths early-return before the header is emitted,
    // so a tool that produces nothing must not leave an orphan label
    // hanging below its tool line.
    setVerboseConfig({ filters: ['all'] });
    const emptyMsg = toolMsg({
      id: 't-verbose-empty',
      result: { status: 'success', output: '' },
    });
    const out = stripAnsi(renderMessageToText(emptyMsg, 'kiro_default'));
    expect(out).not.toContain('output:');
    expect(out).not.toContain('│');
  });

  test('output: header is suppressed when filter excludes the tool', () => {
    // No bar means no header — the gate is the same predicate
    // shouldShowToolOutput uses to decide whether the bar exists at all.
    setVerboseConfig({ filters: ['mcp'] });
    const out = stripAnsi(renderMessageToText(toolMsg(), 'kiro_default'));
    expect(out).not.toContain('output:');
    expect(out).not.toContain('│');
  });
});

// Tool envelopes off the wire are nested ({items:[{Json:{stdout, stderr,
// exit_status}}]} for shell, {content:[{text}]} for read, etc.). The verbose
// output bar must extract the user-meaningful text from these instead of
// dumping the raw envelope JSON.
describe('verbose output envelope unwrapping', () => {
  beforeEach(() => {
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
    // Disable the default output cap (3 lines) for this block — these tests
    // assert the FULL unwrapped body lands in the bar, including multi-line
    // values that would otherwise tail-truncate.
    setVerboseConfig({
      filters: ['all'],
      display: { ...DEFAULT_DISPLAY, outputMaxLines: null },
    });
  });

  afterAll(() => {
    setVerboseConfig({ filters: [] });
    resetVerboseCache();
  });

  const toolMsg = (overrides: Record<string, unknown> = {}) => ({
    id: 't-unwrap-1',
    role: 'tool_use' as const,
    name: 'execute_bash',
    content: JSON.stringify({ command: 'echo hi' }),
    isFinished: true,
    ...overrides,
  });

  const READ_ARGS = JSON.stringify({ operations: [{ path: '/tmp/x' }] });

  it.each<{
    name: string;
    overrides: Record<string, unknown>;
    contains: string[];
    absent?: string[];
    /** Min number of `│` bar rows (multi-line-per-bar regression). */
    minBars?: number;
  }>([
    {
      name: 'shell {items:[{Json:{stdout...}}]} surfaces stdout only on success',
      overrides: {
        result: {
          status: 'success',
          output: {
            items: [
              {
                Json: {
                  exit_status: 'exit status: 0',
                  stdout: 'hello world\n',
                  stderr: '',
                },
              },
            ],
          },
        },
      },
      contains: ['hello world'],
      // No envelope-key leakage, trailing \n trimmed, no implicit (exit 0).
      absent: ['exit_status', '"items"', '"Json"', '\\n', '(exit 0)'],
    },
    {
      name: 'shell multi-line stdout → real newlines, one bar per line',
      overrides: {
        result: {
          status: 'success',
          output: {
            items: [
              {
                Json: {
                  exit_status: 'exit status: 0',
                  stdout: 'line one\nline two\nline three',
                  stderr: '',
                },
              },
            ],
          },
        },
      },
      contains: ['line one', 'line two', 'line three'],
      absent: ['\\n'],
      minBars: 3,
    },
    {
      name: 'shell non-zero exit and stderr both surface',
      overrides: {
        result: {
          status: 'success',
          output: {
            items: [
              {
                Json: {
                  exit_status: 'exit status: 1',
                  stdout: '',
                  stderr: 'something failed',
                },
              },
            ],
          },
        },
      },
      contains: ['(exit 1)', '[stderr] something failed'],
    },
    {
      name: 'read {content:[{text}]} surfaces inner text',
      overrides: {
        name: 'fs_read',
        content: READ_ARGS,
        result: {
          status: 'success',
          output: {
            content: [{ text: 'first line\nsecond line\nthird line' }],
          },
        },
      },
      contains: ['first line', 'second line', 'third line'],
      absent: ['"content"', '"text"', '\\n'],
    },
    {
      name: 'items[0].Text unwraps to the inner string',
      overrides: {
        name: 'fs_read',
        content: READ_ARGS,
        result: {
          status: 'success',
          output: { items: [{ Text: 'plain inner text\nwith newline' }] },
        },
      },
      contains: ['plain inner text', 'with newline'],
      absent: ['"items"', '"Text"'],
    },
    {
      // Reading only items[0] dropped the rest — all items must surface.
      name: 'multi-item Text envelope concatenates every item',
      overrides: {
        name: 'fs_read',
        content: READ_ARGS,
        result: {
          status: 'success',
          output: {
            items: [
              { Text: 'first block' },
              { Text: 'second block' },
              { Json: { text: 'third block' } },
            ],
          },
        },
      },
      contains: ['first block', 'second block', 'third block'],
    },
    {
      name: 'unknown shape falls back to JSON with literal \\n un-escaped',
      overrides: {
        name: 'mcp__some__tool',
        content: JSON.stringify({ query: 'x' }),
        result: {
          status: 'success',
          output: { weird_field: 'a\nb\nc', other: 42 },
        },
      },
      contains: ['weird_field', 'a', 'b', 'c'],
      absent: ['\\n'],
    },
    {
      name: 'plain string output (no envelope) renders verbatim',
      overrides: {
        result: { status: 'success', output: 'hi from stdout\nsecond line' },
      },
      contains: ['hi from stdout', 'second line'],
    },
    {
      // Empty stdout + zero exit + no stderr → bar renders nothing; no crash/leak.
      name: 'shell envelope with missing stdout/stderr does not crash',
      overrides: {
        result: {
          status: 'success',
          output: { items: [{ Json: { exit_status: 'exit status: 0' } }] },
        },
      },
      contains: ['execute_bash'],
      absent: ['"items"'],
    },
  ])('$name', ({ overrides, contains, absent, minBars }) => {
    const out = stripAnsi(
      renderMessageToText(toolMsg(overrides), 'kiro_default')
    );
    for (const c of contains) expect(out).toContain(c);
    for (const a of absent ?? []) expect(out).not.toContain(a);
    if (minBars != null) {
      const barCount = out.split('\n').filter((l) => l.includes('│')).length;
      expect(barCount).toBeGreaterThanOrEqual(minBars);
    }
  });
});

describe('truncation caps (argsMaxLines / outputMaxLines)', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  afterAll(restoreFullDefaults);

  const buildToolMsg = (output: string) => ({
    id: 't-cap-1',
    role: 'tool_use' as const,
    name: 'execute_bash',
    content: JSON.stringify({ command: 'echo hi' }),
    isFinished: true,
    result: { status: 'success', output },
  });

  // All cap tests share one fully-populated display baseline (every flag on,
  // argsMaxChars=80, every other cap unbounded) and patch only the cap(s)
  // under test, so each row reads as "this cap, this expectation".
  const BASE_DISPLAY: VerboseDisplayConfig = {
    showToolReasoning: true,
    toolArgsMode: 'block',
    showElapsed: true,
    subagent: {
      pipeline: true,
      prompts: true,
      roles: true,
      deps: true,
      responses: true,
    },
    showThinkingContent: true,
    showWriteDiffs: true,
    showTasks: true,
    argsMaxLines: null,
    outputMaxLines: null,
    argsMaxChars: 80,
    outputMaxChars: null,
  };
  const setDisplay = (overrides: Partial<VerboseDisplayConfig>) =>
    setVerboseConfig({ display: { ...BASE_DISPLAY, ...overrides } });

  test('outputMaxLines=10 truncates a 30-line output and emits a marker', () => {
    setDisplay({ outputMaxLines: 10 });
    const lines30 = Array.from({ length: 30 }, (_, i) => `out-${i}`).join('\n');
    const out = stripAnsi(
      renderMessageToText(buildToolMsg(lines30), 'kiro_default')
    );
    // Bar lines have a `│ ` prefix; count the visible bar rows.
    const barLines = out.split('\n').filter((l) => l.includes('│'));
    // Exactly 10 bar rows for output + 1 marker row.
    expect(barLines.length).toBe(11);
    // Tail-window: the LAST 10 source lines (out-20 .. out-29) survive,
    // and the leading 20 are summarized by the marker above. This matches
    // the live-streaming behavior — the user watches output scroll past
    // and the visible window settles on the tail by the time the tool
    // finishes.
    expect(out).toContain('out-20');
    expect(out).toContain('out-29');
    expect(out).not.toContain('out-0');
    expect(out).not.toContain('out-19');
    // Marker names the dropped count (30 - 10 = 20) and indicates "above"
    // so users know content scrolled off the top, not the bottom.
    expect(out).toMatch(/\.\.\. \(truncated; \+20 more lines above\)/);
    // Marker row appears BEFORE the kept rows in the output text.
    const markerIdx = out.indexOf('+20 more lines above');
    const out20Idx = out.indexOf('out-20');
    expect(markerIdx).toBeGreaterThan(-1);
    expect(out20Idx).toBeGreaterThan(markerIdx);
  });

  test('outputMaxLines=null renders all lines with no marker', () => {
    setDisplay({ outputMaxLines: null });
    const lines = Array.from({ length: 8 }, (_, i) => `out-${i}`).join('\n');
    const out = stripAnsi(
      renderMessageToText(buildToolMsg(lines), 'kiro_default')
    );
    expect(out).toContain('out-0');
    expect(out).toContain('out-7');
    expect(out).not.toMatch(/truncated/);
  });

  test('outputMaxLines does not fire when source line count fits the cap', () => {
    setDisplay({ outputMaxLines: 50 });
    const lines = Array.from({ length: 5 }, (_, i) => `out-${i}`).join('\n');
    const out = stripAnsi(
      renderMessageToText(buildToolMsg(lines), 'kiro_default')
    );
    expect(out).toContain('out-0');
    expect(out).toContain('out-4');
    expect(out).not.toMatch(/truncated/);
  });

  test('cap counts logical (source) lines — long lines no longer multiply against the cap', () => {
    setDisplay({ outputMaxLines: 3 });
    // `formatBarBlock` hard-wraps long source lines so each visual row
    // carries its own `│ ` prefix (visual alignment beats clipboard
    // fidelity for tool output — readers see a consistent left margin
    // even on overflow). After the wrap, each 200-char source line at
    // termCols=40 produces several visual rows, so cap=3 fires hard:
    // most of the input ends up above the tail window.
    const sourceLines = [
      'a'.repeat(200),
      'b'.repeat(200),
      'c'.repeat(200),
      'd'.repeat(200),
      'e'.repeat(200),
    ];
    const out = stripAnsi(
      renderMessageToText(
        buildToolMsg(sourceLines.join('\n')),
        'kiro_default',
        {
          termCols: 40,
        }
      )
    );
    // Cap fires regardless of the exact visual-row count; the marker uses
    // the tail-window "above" phrasing.
    expect(out).toMatch(/\(truncated; \+\d+ more lines above\)/);
  });

  test('argsMaxLines=2 caps the block-args tree with a marker', () => {
    setDisplay({ argsMaxLines: 2 });
    // Deliberately many top-level keys so block-args produces > 2 visual rows.
    const content = JSON.stringify({
      a: 'one',
      b: 'two',
      c: 'three',
      d: 'four',
      e: 'five',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-args-cap-1',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
          result: { status: 'success', output: 'ok' },
        },
        'kiro_default'
      )
    );
    // First two arg rows survive, later ones don't.
    expect(out).toContain('a: one');
    expect(out).toContain('b: two');
    expect(out).not.toContain('c: three');
    expect(out).not.toContain('e: five');
    // Marker emitted with the dropped count (5 - 2 = 3).
    expect(out).toMatch(/\.\.\. \(truncated; \+3 more lines\)/);
  });

  test('argsMaxChars=10 clips long string values inside block-mode args', () => {
    // Block-mode now respects the per-value char cap. Without this the
    // user can set chars-per-value to 1 and watch the args block render
    // identically — confusing because the chip in inline mode honors it.
    setDisplay({ argsMaxChars: 10 });
    const content = JSON.stringify({
      command: 'this-is-a-pretty-long-shell-command --with --flags',
      path: 'a/very/long/path/to/some/deeply/nested/file.ts',
      // Explicit purpose so the tool-name line shows reasoning rather than
      // falling back to args.command (which would then carry pre-clip text
      // for an unrelated reason).
      __tool_use_purpose: 'demo block-mode arg clipping',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-args-chars-1',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
          result: { status: 'success', output: 'ok' },
        },
        'kiro_default'
      )
    );
    // Both values clip at 10 chars (9 chars + ellipsis).
    expect(out).toMatch(/command:\s*this-is-a…/);
    expect(out).toMatch(/path:\s*a\/very\/lo…/);
    // Pre-clip block-args text doesn't appear in the args block (the args
    // section starts after `demo block-mode arg clipping`). We can't assert
    // on the whole output because the reasoning line still mentions the
    // tool — but each long value MUST be clipped under the args header.
    const argsBlock = out.split('demo block-mode arg clipping')[1] ?? '';
    expect(argsBlock).not.toContain('pretty-long-shell-command');
    expect(argsBlock).not.toContain('deeply/nested/file.ts');
  });

  test('argsMaxChars=null leaves long string values intact', () => {
    setDisplay({ argsMaxChars: null });
    const content = JSON.stringify({
      command: 'echo hello-world-from-the-other-side',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-args-chars-2',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
          result: { status: 'success', output: 'ok' },
        },
        'kiro_default'
      )
    );
    expect(out).toContain('hello-world-from-the-other-side');
  });

  test('argsMaxLines=null leaves the args block untouched', () => {
    setDisplay({ argsMaxLines: null });
    const content = JSON.stringify({ a: 'one', b: 'two', c: 'three' });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-args-cap-2',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
          result: { status: 'success', output: 'ok' },
        },
        'kiro_default'
      )
    );
    expect(out).toContain('a: one');
    expect(out).toContain('b: two');
    expect(out).toContain('c: three');
    expect(out).not.toMatch(/truncated/);
  });

  test('argsMaxLines=null + multi-line value: NO per-value truncation marker', () => {
    // P438130055: the user-facing "unlimited" toggle saves argsMaxLines=null
    // but a hardcoded MULTI_LINE_VISIBLE = 5 inside formatArgLines still
    // clipped each multi-line string at 5 lines + a "(+N more lines)"
    // delta marker. That made "unlimited" a lie for any tool with a
    // multi-line arg (shell heredocs, long scripts, multi-line patches).
    //
    // The fix propagates argsMaxLines=null through to perValueLineCap so
    // the multi-line clamp is also lifted — no marker should appear.
    // argsMaxChars=null too so the per-line char cap can't accidentally
    // truncate a single line and look like the bug.
    setDisplay({ argsMaxLines: null, argsMaxChars: null });
    const fifty = Array.from({ length: 50 }, (_, i) => `line${i}`).join('\n');
    const content = JSON.stringify({ command: fifty });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-unlimited-multiline',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
          result: { status: 'success', output: 'ok' },
        },
        'kiro_default'
      )
    );
    // None of the truncation markers — neither the per-value
    // "(+N more lines)" nor the block-level "(truncated; +N more lines)".
    expect(out).not.toMatch(/more lines/);
    expect(out).not.toMatch(/truncated/);
    // First, middle, and last source lines all visible in scrollback.
    expect(out).toContain('line0');
    expect(out).toContain('line25');
    expect(out).toContain('line49');
  });

  test('argsMaxLines=N + multi-line value: block-level marker reports ALL hidden source lines, not just dropped visual rows', () => {
    // Follow-up to P438130055. The original fix kept a per-value 5-line
    // cap when argsMaxLines was finite — fairness for multi-arg tools.
    // Catch: with both caps active, the per-value cap emits its own
    // "(+N more lines)" marker, and applyLineCap then chops that marker
    // off as one dropped row — so the block-level marker says "+1 more
    // lines" even though dozens of source lines are hidden underneath.
    //
    // Fix: drop the per-value cap from block mode entirely. The block-
    // level applyLineCap is the single source of truth, and its marker
    // counts visual rows that ARE source lines (since formatArgLines no
    // longer collapses values internally). With argsMaxLines=5 and a
    // 32-line value (cat <<EOF + 30 lines + EOF), the user sees
    // command-head + 4 lines + "(truncated; +27 more lines)" — math
    // checks out: 5 visible, 27 hidden, 32 total.
    setDisplay({ argsMaxLines: 5, argsMaxChars: null });
    // 32-line value mirrors the user-reported case: cat <<EOF + 30
    // numbered lines + EOF. The exact count matters because the
    // assertion below pins the marker to "+27".
    const lines32 = [
      'cat <<EOF >> /tmp/test-banner.txt',
      ...Array.from({ length: 30 }, (_, i) => `line ${i + 1}`),
      'EOF',
    ].join('\n');
    const content = JSON.stringify({ command: lines32 });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-stacked-marker-bug',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
          result: { status: 'success', output: 'ok' },
        },
        'kiro_default'
      )
    );
    // Block-level marker reports 27 hidden — NOT the misleading "+1"
    // that the pre-fix code emitted because the per-value marker got
    // dropped silently into applyLineCap's "+1 row dropped" count.
    expect(out).toMatch(/\.\.\. \(truncated; \+27 more lines\)/);
    expect(out).not.toMatch(/\(truncated; \+1 more lines\)/);
    // The pre-fix per-value marker (with 27 hidden) would also have
    // been emitted but then dropped — guard against it accidentally
    // re-appearing in the visible portion.
    expect(out).not.toMatch(/\.\.\. \(\+\d+ more lines\)/);
    // First arg-block content rows still visible.
    expect(out).toContain('cat <<EOF');
    expect(out).toContain('line 1');
    expect(out).toContain('line 4');
    // Anything beyond the cap is hidden.
    expect(out).not.toContain('line 5');
    expect(out).not.toContain('EOF\n');
  });

  test('argsMaxLines=N + multi-line value: marker count matches (totalSourceLines - cap)', () => {
    // Tighter formulation of the rule above. argsMaxLines=10, value with
    // 50 source lines: 10 visible (head + 9 tail rows = 1 + 9 source
    // lines), 40 hidden. Marker reports 40, not 1.
    setDisplay({ argsMaxLines: 10, argsMaxChars: null });
    const fifty = Array.from({ length: 50 }, (_, i) => `line${i}`).join('\n');
    const content = JSON.stringify({ command: fifty });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-stacked-marker-precise',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
          result: { status: 'success', output: 'ok' },
        },
        'kiro_default'
      )
    );
    expect(out).toMatch(/\.\.\. \(truncated; \+40 more lines\)/);
    // argsMaxLines=10 keeps 10 visual rows: head (line0) + line1..line9.
    // line10..line49 land in the dropped 40-row tail.
    expect(out).toContain('line0');
    expect(out).toContain('line9');
    expect(out).not.toContain('line10');
    expect(out).not.toContain('line49');
  });

  test('pathologically long single line is clipped before wrap (no OOM)', () => {
    // Regression for the "RangeError: Out of memory" crash a user hit in the
    // wild. The agent ran a `grep -E` shell command whose stdout contained
    // a single match against a minified bundle / binary file — one logical
    // line many MB long. The lite renderer's formatBarBlock walked every
    // code point of that line through wrapAnsiLine, allocating a
    // `{ansi, ch, width}` cell object per code point — tens of millions of
    // objects, several GB of heap, OOM. The downstream applyTailLineCap
    // (which would have discarded most rows) only runs AFTER formatBarBlock
    // returns, so it can't help.
    //
    // formatBarBlock now clips each input line to MAX_INPUT_LINE_CHARS
    // (200_000) chars, keeping the TAIL so it composes correctly with the
    // tail-keep applyTailLineCap downstream. A 1MB single line should now
    // render as a clip marker + the last 200K chars wrapped normally,
    // without exhausting heap.
    setDisplay({ outputMaxLines: null }); // unbounded: prove it doesn't OOM regardless
    // 1MB single-line payload — same shape as `grep`-matching a minified
    // bundle. Use a printable filler so the visible-width math doesn't add
    // surprise factors on top of the OOM-prevention assertion.
    const huge = 'x'.repeat(1_000_000);
    const out = stripAnsi(
      renderMessageToText(buildToolMsg(huge), 'kiro_default', {
        termCols: 80,
      })
    );
    // Clip marker is emitted before the wrapped tail. 1_000_000 -
    // 200_000 = 800_000 chars hidden.
    expect(out).toMatch(/\.\.\. \(line clipped; \+800000 chars before\)/);
    // The bar rows that survived are bounded — far below what the
    // pre-fix path would have produced on this input. 200_000 chars
    // wrapped at 80 cols yields ~2500 rows + 1 marker. Exercise the
    // upper bound as a tripwire: if a future change blows past this,
    // we want a loud test failure, not a silent regression toward
    // O(input size) memory growth.
    const barLines = out.split('\n').filter((l) => l.includes('│'));
    expect(barLines.length).toBeLessThan(3000);
    // Tail is preserved — last char of the input is the last char of the
    // last bar row (modulo the trailing tool-call newline).
    const lastBar = barLines[barLines.length - 1] ?? '';
    expect(lastBar.endsWith('x')).toBe(true);
  }, 30_000);

  test('lines under MAX_INPUT_LINE_CHARS are not clipped', () => {
    // Companion to the OOM regression: confirm the per-line cap is high
    // enough that a "long but plausible" output (here 50K chars on a
    // single line — well past any normal terminal width but realistic for
    // a JSON blob, a stack trace, an interpreter error) renders without
    // the clip marker. Without this, a future tightening of
    // MAX_INPUT_LINE_CHARS could make legitimate output mysteriously
    // grow a "(line clipped)" marker.
    setDisplay({ outputMaxLines: null });
    const long = 'y'.repeat(50_000);
    const out = stripAnsi(
      renderMessageToText(buildToolMsg(long), 'kiro_default', {
        termCols: 80,
      })
    );
    expect(out).not.toContain('(line clipped');
  });

  // Write diffs deliberately OPT OUT of outputMaxLines: they materialize
  // whole at finish time and the entire change is the payload the user is
  // reviewing, so even with a cap set the diff body renders in full with no
  // truncation marker (read tool bodies still honor the cap). See d26327c03.
  // Complements the null-cap sibling below by proving the opt-out holds even
  // when a cap IS configured.
  test('outputMaxLines does not cap fs_write create diff (write diffs opt out)', () => {
    setDisplay({ outputMaxLines: 5 });
    // 30-line file create with outputMaxLines=5 set: the cap must NOT
    // apply — all 30 added lines render and no truncation marker appears.
    const longContent = Array.from({ length: 30 }, (_, i) => `line-${i}`).join(
      '\n'
    );
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-write-cap',
          role: 'tool_use',
          name: 'fs_write',
          content: JSON.stringify({
            command: 'create',
            path: 'src/big.ts',
            content: longContent,
          }),
          isFinished: true,
          result: {
            status: 'success',
            output: 'Successfully created src/big.ts (30 lines).',
          },
        } as any,
        'kiro_default'
      )
    );
    expect(out).toContain('fs_write');
    // Every line survives — head and tail — because write diffs opt out.
    expect(out).toMatch(/\+\s+line-0/);
    expect(out).toMatch(/\+\s+line-29/);
    // No truncation marker: the cap does not touch write diff bodies.
    expect(out).not.toMatch(/\.\.\. \(truncated; \+\d+ more lines\)/);
  });

  test('outputMaxLines null leaves write diff uncapped', () => {
    setDisplay({ outputMaxLines: null });
    const content = Array.from({ length: 20 }, (_, i) => `line-${i}`).join(
      '\n'
    );
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-write-uncapped',
          role: 'tool_use',
          name: 'fs_write',
          content: JSON.stringify({
            command: 'create',
            path: 'src/full.ts',
            content,
          }),
          isFinished: true,
          result: { status: 'success' },
        } as any,
        'kiro_default'
      )
    );
    // All 20 lines render; no truncation marker.
    expect(out).toContain('line-0');
    expect(out).toContain('line-19');
    expect(out).not.toMatch(/\(truncated; \+/);
  });

  // The literal `Successfully created/replaced/inserted ...` string the
  // agent returns for write tools duplicates what the diff already shows.
  // Dropping it from scrollback was the second half of the merge ("write
  // diff IS the write output"). Errors still surface — a separate test
  // pins the failure path.
  test('successful fs_write does not render the redundant success line', () => {
    setVerboseConfig({ filters: ['all'] }); // even with all filters on
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-write-success-line',
          role: 'tool_use',
          name: 'fs_write',
          content: JSON.stringify({
            command: 'create',
            path: 'src/foo.ts',
            content: 'hello',
          }),
          isFinished: true,
          result: {
            status: 'success',
            output: 'Successfully created src/foo.ts (1 lines).',
          },
        } as any,
        'kiro_default'
      )
    );
    // Diff body present.
    expect(out).toMatch(/\+\s+hello/);
    // Success chrome line dropped — even with `filters: ['all']` it must
    // not show up. Guards against a regression where the call site falls
    // back to renderVerboseOutput unconditionally.
    expect(out).not.toContain('Successfully created');
  });

  test('failed fs_write still surfaces its error under the diff', () => {
    setVerboseConfig({ filters: [] }); // filters off — error path bypasses
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-write-error',
          role: 'tool_use',
          name: 'fs_write',
          content: JSON.stringify({
            command: 'create',
            path: '/readonly/foo.ts',
            content: 'hi',
          }),
          isFinished: true,
          result: {
            status: 'error',
            error: 'permission denied: /readonly/foo.ts',
          },
        } as any,
        'kiro_default'
      )
    );
    // Error block renders below the diff regardless of filter state.
    expect(out).toContain('permission denied');
  });
});

describe('pretty-printed tool output (json envelopes)', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: ['all'] });
  });

  afterAll(restoreFullDefaults);

  const buildJsonOutputMsg = (output: unknown) => ({
    id: 't-json-1',
    role: 'tool_use' as const,
    name: 'mcp__some-server__lookup',
    content: JSON.stringify({ query: 'find me a thing' }),
    isFinished: true,
    result: { status: 'success', output },
  });

  test('plain string output still renders as a flat bar block (text path unchanged)', () => {
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-text-1',
          role: 'tool_use',
          name: 'execute_bash',
          content: JSON.stringify({ command: 'echo hi' }),
          isFinished: true,
          result: { status: 'success', output: 'line-a\nline-b' },
        },
        'kiro_default'
      )
    );
    // No `key:` shape, just the literal lines under the bar.
    expect(out).toContain('│ line-a');
    expect(out).toContain('│ line-b');
    expect(out).not.toMatch(/key:/);
  });

  test('unknown json envelope renders as key:value tree under the bar', () => {
    // No items / content / shell keys — falls through to the json path.
    const out = stripAnsi(
      renderMessageToText(
        buildJsonOutputMsg({
          status: 'ok',
          count: 3,
          query: 'find me a thing',
        }),
        'kiro_default'
      )
    );
    // Each top-level key surfaces with bar prefix + `key: value`.
    expect(out).toContain('│ status: ok');
    expect(out).toContain('│ count: 3');
    expect(out).toContain('│ query: find me a thing');
    // The raw JSON brace form must NOT appear — that's the regression we
    // were trying to kill.
    expect(out).not.toContain('{"status":"ok"');
  });

  test('items[].Json envelope with unknown inner shape pretty-prints the inner object', () => {
    // Real MCP shape: { items: [{ Json: { ...structured fields } }] }.
    // None of the known string keys (text, content, stdout) are present, so
    // the inner Json object should reach the pretty-printer.
    const out = stripAnsi(
      renderMessageToText(
        buildJsonOutputMsg({
          items: [{ Json: { matches: 5, latency_ms: 42 } }],
        }),
        'kiro_default'
      )
    );
    expect(out).toContain('│ matches: 5');
    expect(out).toContain('│ latency_ms: 42');
  });

  test('deeply nested json (>4 levels) does NOT collapse to safeJson', () => {
    // The args path caps at maxDepth=4; the output path must not, since
    // the user already controls footprint via outputMaxLines.
    const out = stripAnsi(
      renderMessageToText(
        buildJsonOutputMsg({
          a: { b: { c: { d: { e: { f: 'deep-value' } } } } },
        }),
        'kiro_default'
      )
    );
    // The leaf value at depth 6 must be reachable as a `key: value` row,
    // not buried in a `{...}` collapsed JSON dump.
    expect(out).toContain('f: deep-value');
    expect(out).not.toMatch(/\{"f":"deep-value"\}/);
  });

  test('outputMaxLines applies to the json tree the same way it does to text', () => {
    setVerboseConfig({
      display: {
        showToolReasoning: true,
        toolArgsMode: 'block',
        showElapsed: true,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: 3,
        argsMaxChars: 80,
        outputMaxChars: null,
      },
    });
    // 10 top-level keys → 10 rows pre-cap. Cap at 3 should drop 7.
    const big: Record<string, number> = {};
    for (let i = 0; i < 10; i++) big[`k${i}`] = i;
    const out = stripAnsi(
      renderMessageToText(buildJsonOutputMsg(big), 'kiro_default')
    );
    // Tail-window: the LAST 3 rows survive (k7/k8/k9), leading 7 hidden
    // behind the marker above. Mirrors text-output behavior — both paths
    // funnel through applyTailLineCap so the user sees consistent
    // truncation across structured and unstructured tool output.
    expect(out).toContain('k7: 7');
    expect(out).toContain('k9: 9');
    expect(out).not.toContain('k0: 0');
    expect(out).not.toContain('k6: 6');
    expect(out).toMatch(/\(truncated; \+7 more lines above\)/);
  });

  test('error path on a json envelope is unchanged (red bar of error text)', () => {
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-err-1',
          role: 'tool_use',
          name: 'mcp__some-server__lookup',
          content: JSON.stringify({ query: 'q' }),
          isFinished: true,
          result: {
            status: 'error',
            error: 'something went wrong',
            output: { items: [{ Json: { ignored: true } }] },
          },
        },
        'kiro_default'
      )
    );
    expect(out).toContain('│ something went wrong');
    // The structured output must NOT leak when we have an explicit error.
    expect(out).not.toContain('ignored: true');
  });

  test('json-shape tool output bodies pick up the same green tint as text-shape outputs', () => {
    // The sage-green tint (#a3c0a3) is the "successful result" outcome
    // signal for tool-output bar blocks. Text-shape outputs (shell, grep,
    // anything that returns a plain string) get it via formatBarBlock;
    // JSON-shape envelopes (most MCP tools, knowledge searches, code-
    // intel results) used to render plain — same successful result, two
    // different visual treatments. This test pins that the JSON path now
    // wraps body lines with the same body color so the tint is consistent
    // across both branches.
    //
    // We assert the SGR open code (chalk truecolor `\x1b[38;2;...m`) for
    // the body hex appears in the output. Stripping ANSI would erase the
    // exact thing we're trying to verify, so we keep the raw output and
    // pattern-match the SGR.
    const greenSgr = '\x1b[38;2;163;192;163m';
    const jsonOut = renderMessageToText(
      buildJsonOutputMsg({ status: 'ok', count: 3 }),
      'kiro_default'
    );
    expect(jsonOut).toContain(greenSgr);
    // Sanity check: the same SGR is on the text-shape path so the test
    // is comparing apples-to-apples — if shell output ever loses the
    // tint, this assertion catches it before the json-shape one does.
    const shellOut = renderMessageToText(
      {
        id: 't-text-tint',
        role: 'tool_use',
        name: 'execute_bash',
        content: JSON.stringify({ command: 'echo hi' }),
        isFinished: true,
        result: { status: 'success', output: 'hello' },
      },
      'kiro_default'
    );
    expect(shellOut).toContain(greenSgr);
  });
});

describe('display.toolArgsMode rendering', () => {
  beforeEach(() => {
    resetVerboseCache();
  });

  const buildToolMsg = () => ({
    id: 't-args-1',
    role: 'tool_use' as const,
    name: 'shell',
    content: JSON.stringify({
      command: 'git status',
      working_dir: '/tmp/repo',
      __tool_use_purpose: 'check git state',
    }),
    isFinished: true,
    result: { status: 'success', output: 'on branch main' },
  });

  const BASE_DISPLAY: VerboseDisplayConfig = {
    showToolReasoning: true,
    toolArgsMode: 'block',
    showElapsed: true,
    subagent: {
      pipeline: true,
      prompts: true,
      roles: true,
      deps: true,
      responses: true,
    },
    showThinkingContent: true,
    showWriteDiffs: true,
    showTasks: true,
    argsMaxLines: null,
    outputMaxLines: null,
    argsMaxChars: 80,
    outputMaxChars: null,
  };
  const setDisplay = (overrides: Partial<VerboseDisplayConfig>) =>
    setVerboseConfig({ display: { ...BASE_DISPLAY, ...overrides } });

  // Each row toggles the args presentation and asserts what the header shows.
  // 'inline' also flips reasoning off so the chip stands in for the args.
  test.each([
    [
      'block renders the full key:value tree (default)',
      { toolArgsMode: 'block' as const },
      [
        'shell',
        'check git state',
        'command: git status',
        'working_dir: /tmp/repo',
      ],
      [],
    ],
    [
      'off hides args entirely; reasoning still shows',
      { toolArgsMode: 'off' as const },
      ['shell', 'check git state'],
      ['command: git status', 'working_dir'],
    ],
    [
      'inline + reasoning off shows tool [arg] chip',
      { toolArgsMode: 'inline' as const, showToolReasoning: false },
      ['shell', '[git status]'],
      ['check git state', 'working_dir: /tmp/repo'],
    ],
  ])('toolArgsMode %s', (_name, overrides, contains, absent) => {
    setDisplay(overrides);
    const out = stripAnsi(renderMessageToText(buildToolMsg(), 'kiro_default'));
    for (const c of contains) expect(out).toContain(c);
    for (const a of absent) expect(out).not.toContain(a);
  });

  test('showElapsed false strips the duration tail', () => {
    setDisplay({ showElapsed: false });
    const msg = {
      ...buildToolMsg(),
      startTime: 0,
      finishTime: 1500,
    };
    const out = stripAnsi(renderMessageToText(msg, 'kiro_default'));
    expect(out).toContain('shell');
    expect(out).not.toContain('1.5s');
    expect(out).not.toContain('1500ms');
  });

  test('showToolReasoning false drops the purple "why" segment from the header', () => {
    setDisplay({ showToolReasoning: false });
    const out = stripAnsi(renderMessageToText(buildToolMsg(), 'kiro_default'));
    expect(out).toContain('shell');
    expect(out).not.toContain('check git state');
  });
});

// Inline-mode chip extraction. The /verbosity inline mode renders one chip
// per tool call as `tool [arg]` — this section locks in WHICH arg gets
// chosen for each tool kind. The legacy code's priority list put `path`
// before `pattern`/`query`, which made grep/glob calls always show the
// path (often `.` or a directory the user already knew) and hide the
// actually-informative pattern. The new logic combines pattern+path with
// " in ", verb-prefixes write tools, and shortens absolute paths to
// cwd-relative or `~`-substituted form.
describe('inline arg chip — pattern/path combination + path shortening', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({
      display: {
        showToolReasoning: false,
        toolArgsMode: 'inline',
        showElapsed: false,
        subagent: {
          pipeline: true,
          prompts: true,
          roles: true,
          deps: true,
          responses: true,
        },
        showThinkingContent: true,
        showTasks: true,
        argsMaxLines: null,
        outputMaxLines: null,
        argsMaxChars: 200,
        outputMaxChars: null,
      },
    });
  });

  afterAll(() => {
    setVerboseConfig({ filters: [] });
    resetVerboseCache();
  });

  const toolMsg = (name: string, args: Record<string, unknown>) => ({
    id: `t-inline-${name}-${Math.random()}`,
    role: 'tool_use' as const,
    name,
    content: JSON.stringify(args),
    isFinished: true,
    result: { status: 'success', output: 'ok' },
  });

  // `CWD` in an args path / contains / absent string is substituted with
  // process.cwd() (the repo TUI dir) at run time so the cwd-shortening cases
  // can assert the absolute prefix is stripped.
  const cwd = process.cwd();
  const sub = (s: string) => s.replace('CWD', cwd);

  it.each<{
    name: string;
    tool: string;
    args: Record<string, unknown>;
    contains: string;
    absent?: string[];
    /** Assert cwd doesn't leak — whole output, or only the chip slice. */
    noCwd?: 'output' | 'chip';
  }>([
    {
      name: 'grep pattern + relative path → "pattern in path"',
      tool: 'grep',
      args: { pattern: 'wrapAnsiLine', path: 'packages/tui/src' },
      contains: 'grep [wrapAnsiLine in packages/tui/src]',
    },
    {
      name: 'grep absolute path inside cwd → cwd-relative',
      tool: 'grep',
      args: { pattern: 'foo', path: 'CWD/src/lite' },
      contains: 'grep [foo in src/lite]',
      noCwd: 'output',
    },
    {
      name: 'grep pattern only drops the " in path" suffix',
      tool: 'grep',
      args: { pattern: 'wrapAnsiLine' },
      contains: 'grep [wrapAnsiLine]',
      absent: [' in '],
    },
    {
      name: 'grep path "." dropped (adds no info)',
      tool: 'grep',
      args: { pattern: 'foo', path: '.' },
      contains: 'grep [foo]',
      absent: [' in ', '[foo in .'],
    },
    {
      name: 'glob pattern + path → " in "',
      tool: 'glob',
      args: { pattern: '**/*.tsx', path: 'src/components' },
      contains: 'glob [**/*.tsx in src/components]',
    },
    {
      name: 'fs_write strReplace → "edit <path>" (not the discriminator)',
      tool: 'fs_write',
      args: {
        command: 'strReplace',
        path: 'src/lite/render.ts',
        oldStr: 'foo',
        newStr: 'bar',
      },
      contains: 'fs_write [edit src/lite/render.ts]',
      absent: ['[strReplace]'],
    },
    {
      name: 'fs_write create → "create <path>"',
      tool: 'fs_write',
      args: { command: 'create', path: 'src/foo.ts', content: 'hello' },
      contains: 'fs_write [create src/foo.ts]',
    },
    {
      name: 'fs_write insert → "insert <path>"',
      tool: 'fs_write',
      args: {
        command: 'insert',
        path: 'src/foo.ts',
        insertLine: 5,
        content: 'hello',
      },
      contains: 'fs_write [insert src/foo.ts]',
    },
    {
      name: 'fs_write delete → "delete <path>"',
      tool: 'fs_write',
      args: { command: 'delete', path: 'src/foo.ts' },
      contains: 'fs_write [delete src/foo.ts]',
    },
    {
      // The diff body below the tool line legitimately shows the full path;
      // only the chip slice must be cwd-free, hence noCwd: 'chip'.
      name: 'fs_write absolute path inside cwd shortened in chip',
      tool: 'fs_write',
      args: {
        command: 'strReplace',
        path: 'CWD/src/foo.ts',
        oldStr: 'a',
        newStr: 'b',
      },
      contains: 'fs_write [edit src/foo.ts]',
      noCwd: 'chip',
    },
    {
      // Legacy "command means shell" branch, gated to SHELL_TOOL_NAMES so
      // fs_write's `command` discriminator can't hijack it.
      name: 'shell shows the command',
      tool: 'shell',
      args: { command: 'git status' },
      contains: 'shell [git status]',
    },
    {
      name: 'read bare path renders shortened',
      tool: 'fs_read',
      args: { operations: [{ path: 'CWD/src/lite/render.ts' }] },
      contains: '[src/lite/render.ts]',
      noCwd: 'output',
    },
  ])('$name', ({ tool, args, contains, absent, noCwd }) => {
    const resolvedArgs = JSON.parse(sub(JSON.stringify(args)));
    const out = stripAnsi(
      renderMessageToText(toolMsg(tool, resolvedArgs), 'kiro_default')
    );
    expect(out).toContain(contains);
    for (const a of absent ?? []) expect(out).not.toContain(a);
    if (noCwd === 'output') {
      expect(out).not.toContain(cwd);
    } else if (noCwd === 'chip') {
      const chipMatch = out.match(/fs_write \[([^\]]+)\]/);
      expect(chipMatch).not.toBeNull();
      expect(chipMatch![1]).not.toContain(cwd);
    }
  });
});
