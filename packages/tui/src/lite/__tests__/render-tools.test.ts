import './setup-chalk-level.js';

import { describe, test, it, expect, beforeEach, afterAll } from 'vitest';
import chalk from 'chalk';
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
import { useTempKiroHome } from './temp-kiro-home.js';

useTempKiroHome();

// Restore the all-flags-on display (unbounded output, 80-char arg cap) so a
// suite that mutated the global verbose config doesn't leak into the next file.
function restoreFullDefaults() {
  setVerboseConfig({
    filters: [],
    display: {
      ...DEFAULT_DISPLAY,
      subagent: { ...DEFAULT_DISPLAY.subagent },
      outputMaxLines: null,
      argsMaxChars: 80,
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
  // Multi-line `command` value rendering: argsMaxChars clips per-line only
  // (decoupled from line count); perValueLineCap bounds visible source lines
  // with a "+N more lines" DELTA marker (not a total count). null lifts the
  // line cap entirely (P438130055: "unlimited" toggle propagates here so a
  // 50-line heredoc renders fully). undefined keeps the historical 5-line
  // default for callers (ApprovalPrompt, output bar) that don't pass it.
  // `lineN` helper: a multi-line string of `line0..line{N-1}`.
  const lineN = (n: number) =>
    JSON.stringify({
      command: Array.from({ length: n }, (_, i) => `line${i}`).join('\n'),
    });
  it.each<{
    name: string;
    content: string;
    maxChars: number | null;
    perValueLineCap?: number | null;
    contains?: string[];
    absent?: string[];
    matches?: RegExp[];
    notMatches?: RegExp[];
  }>([
    {
      // 7-line value, default cap → line0..line4 visible (line0 on the head
      // row with the key), line5/line6 hidden behind the marker.
      name: '7-line value with argsMaxChars set: 5 visible, not collapsed to 1',
      content: lineN(7),
      maxChars: 120,
      contains: ['line1', 'line4'],
      absent: ['line5', 'line6'],
      matches: [/command:\s*line0/],
    },
    {
      // Delta = 50 - 5 = 45 hidden. The pre-fix total-count form "(50 lines)"
      // MUST NOT appear (it falsely implied 50 hidden when only 45 were).
      name: 'marker uses delta count (+45 more lines), not total',
      content: lineN(50),
      maxChars: 120,
      matches: [/\.\.\. \(\+45 more lines\)/],
      notMatches: [/\(50 lines\)/],
    },
    {
      name: 'marker omitted when value fits in 5 lines',
      content: JSON.stringify({ command: ['a', 'b', 'c', 'd'].join('\n') }),
      maxChars: 120,
      contains: ['a', 'd'],
      notMatches: [/more lines/],
    },
    {
      // null perValueLineCap (unlimited): all 50 lines, no marker.
      name: 'renders all lines when perValueLineCap is null',
      content: lineN(50),
      maxChars: null,
      perValueLineCap: null,
      contains: ['line0', 'line25', 'line49'],
      notMatches: [/more lines/],
    },
    {
      // Default cap when perValueLineCap omitted: 10 - 5 = 5 hidden.
      name: 'defaults to a 5-line per-value cap (back-compat)',
      content: lineN(10),
      maxChars: 120,
      contains: ['line0', 'line4'],
      absent: ['line5'],
      matches: [/\.\.\. \(\+5 more lines\)/],
    },
    {
      // Explicit cap is a visible-line count: 50 - 3 = 47 hidden.
      name: 'explicit perValueLineCap of 3 clips a 50-line value to 3 + marker',
      content: lineN(50),
      maxChars: null,
      perValueLineCap: 3,
      contains: ['line0', 'line2'],
      absent: ['line3'],
      matches: [/\.\.\. \(\+47 more lines\)/],
    },
  ])('multi-line $name', (c) => {
    const lines =
      c.perValueLineCap === undefined
        ? formatToolArgLines('shell', c.content, 120, c.maxChars)
        : formatToolArgLines(
            'shell',
            c.content,
            120,
            c.maxChars,
            c.perValueLineCap
          );
    expect(lines).not.toBeNull();
    const joined = (lines ?? []).map(stripAnsi).join('\n');
    for (const s of c.contains ?? []) expect(joined).toContain(s);
    for (const s of c.absent ?? []) expect(joined).not.toContain(s);
    for (const re of c.matches ?? []) expect(joined).toMatch(re);
    for (const re of c.notMatches ?? []) expect(joined).not.toMatch(re);
  });

  test('argsMaxChars clips EACH line of a multi-line value, not just the head', () => {
    // Per-line clip is what the knob's name says. Without this, only the
    // first line was clipped (when collapse fired) and continuation rows
    // rendered uncapped — confusing inconsistency. Kept standalone: asserts
    // per-row position (head vs continuation), not just substring presence.
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
});

describe('formatTaskToolBody', () => {
  // Happy-path command cases share the shape: parse content, assert .command
  // and contains/absent substrings on the stripped bodyLines. Structural cases
  // (connector glyphs, details indent, wrap width) stay standalone below.
  it.each<{
    name: string;
    content: unknown;
    command?: string;
    isNull?: boolean;
    contains?: string[];
    absent?: string[];
    absentMatch?: RegExp[];
  }>([
    {
      name: 'create renders a numbered task list with tree connectors',
      content: {
        command: 'create',
        task_list_description: 'Add lite rendering for tasks',
        tasks: [
          { task_description: 'Investigate the schema' },
          { task_description: 'Implement the renderer' },
          { task_description: 'Add tests' },
        ],
      },
      command: 'create',
      // ├─ for non-last rows, └─ for the last.
      contains: [
        'description:',
        'Add lite rendering for tasks',
        '1. Investigate the schema',
        '2. Implement the renderer',
        '3. Add tests',
        '├─',
        '└─',
      ],
    },
    {
      name: 'add renders new_tasks with optional new_description',
      content: {
        command: 'add',
        new_description: 'expanded scope',
        new_tasks: [{ task_description: 'New thing' }],
      },
      command: 'add',
      contains: ['description:', 'expanded scope', '1. New thing'],
    },
    {
      name: 'complete renders ID chips, notes block, and modified files',
      content: {
        command: 'complete',
        completed_task_ids: ['1', '2', '3'],
        context_update:
          'Found that the schema lives in the agent crate and is stable across versions.',
        modified_files: [
          'packages/tui/src/lite/render.ts',
          'packages/tui/src/lite/__tests__/render.test.ts',
        ],
      },
      command: 'complete',
      contains: [
        'completed:',
        '#1',
        '#2',
        '#3',
        'notes:',
        'schema lives in the agent crate',
        'files:',
        '- packages/tui/src/lite/render.ts',
      ],
    },
    {
      // whitespace-only context_update treated as empty.
      name: 'complete without notes or files renders only the ID chips',
      content: {
        command: 'complete',
        completed_task_ids: ['7'],
        context_update: '   ',
      },
      contains: ['completed:', '#7'],
      absent: ['notes:', 'files:'],
    },
    {
      name: 'remove shows IDs and optional new description',
      content: {
        command: 'remove',
        remove_task_ids: ['2', '5'],
        new_description: 'narrowed plan',
      },
      command: 'remove',
      contains: ['removed:', '#2', '#5', 'description:', 'narrowed plan'],
    },
    {
      // Defensive: malformed/partial args must not render a phantom numbered row.
      name: 'skips tasks with empty or missing task_description',
      content: {
        command: 'create',
        task_list_description: 'd',
        tasks: [
          { task_description: 'real one' },
          { task_description: '' },
          { task_description: '   ' },
          {},
        ],
      },
      contains: ['real one'],
      absentMatch: [/\b2\./, /\b3\./, /\b4\./],
    },
  ])('$name', ({ content, command, contains, absent, absentMatch }) => {
    const result = formatTaskToolBody(JSON.stringify(content), 120);
    expect(result).not.toBeNull();
    if (command) expect(result!.command).toBe(command);
    const text = result!.bodyLines.map(stripAnsi).join('\n');
    for (const s of contains ?? []) expect(text).toContain(s);
    for (const s of absent ?? []) expect(text).not.toContain(s);
    for (const re of absentMatch ?? []) expect(text).not.toMatch(re);
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

  // Each row sets a filter, renders one tool message (default = execute_bash
  // with "hi from stdout"), then asserts what the output bar shows/hides. The
  // env-var row exercises the saved-config-wins rule; the "output:" header
  // shares the shouldShowToolOutput gate, so it appears iff the bar does.
  it.each<{
    name: string;
    filters: string[];
    msgOverride?: Record<string, unknown>;
    env?: string;
    contains?: string[];
    absent?: string[];
  }>([
    {
      name: 'verbose off: no output bar but tool name still renders',
      filters: [],
      contains: ['execute_bash'],
      absent: ['hi from stdout', '│', 'output:'],
    },
    {
      name: 'filter "all": full output + bar glyph + output: header',
      filters: ['all'],
      contains: ['hi from stdout', 'second line', '│', 'output:'],
    },
    {
      name: 'filter excludes this tool: no bar and no output: header',
      filters: ['mcp'],
      absent: ['hi from stdout', '│', 'output:'],
    },
    {
      name: 'filter "shell" lets bash through',
      filters: ['shell'],
      contains: ['hi from stdout'],
    },
    {
      name: 'filter "shell" blocks fs_read',
      filters: ['shell'],
      msgOverride: {
        id: 't-verbose-2',
        name: 'fs_read',
        content: JSON.stringify({ operations: [{ path: '/tmp/x' }] }),
        result: { status: 'success', output: 'file contents here' },
      },
      absent: ['file contents here'],
    },
    {
      name: 'exact MCP filter lets the named tool through',
      filters: ['mcp__nova-memory-mcp__recall'],
      msgOverride: {
        id: 't-verbose-3',
        name: 'mcp__nova-memory-mcp__recall',
        content: JSON.stringify({ query: 'history' }),
        result: { status: 'success', output: 'memory blob' },
      },
      contains: ['memory blob'],
    },
    {
      name: 'exact MCP filter blocks a sibling MCP tool',
      filters: ['mcp__nova-memory-mcp__recall'],
      msgOverride: {
        id: 't-verbose-4',
        name: 'mcp__nova-memory-mcp__remember',
        content: JSON.stringify({ messages: [] }),
        result: { status: 'success', output: 'persisted' },
      },
      absent: ['persisted'],
    },
    {
      name: 'error result renders the error text + output: header',
      filters: ['all'],
      msgOverride: {
        id: 't-verbose-err',
        result: { status: 'error', error: 'command failed: exit 1' },
      },
      contains: ['command failed', 'output:'],
    },
    {
      // Env var is only a first-run hint; a persisted config wins, so
      // KIRO_LITE_VERBOSE=1 must NOT force output back on over filters:[].
      name: 'env KIRO_LITE_VERBOSE=1 is a no-op when a config is saved',
      filters: [],
      env: '1',
      absent: ['hi from stdout'],
    },
    {
      // Empty output early-returns before the header, so no orphan label.
      name: 'empty output: no bar and no orphan output: header',
      filters: ['all'],
      msgOverride: {
        id: 't-verbose-empty',
        result: { status: 'success', output: '' },
      },
      absent: ['output:', '│'],
    },
  ])('$name', ({ filters, msgOverride, env, contains, absent }) => {
    setVerboseConfig({ filters });
    if (env !== undefined) {
      process.env.KIRO_LITE_VERBOSE = env;
      resetVerboseCache();
    }
    const out = stripAnsi(
      renderMessageToText(toolMsg(msgOverride), 'kiro_default')
    );
    for (const s of contains ?? []) expect(out).toContain(s);
    for (const s of absent ?? []) expect(out).not.toContain(s);
  });

  // The dim "output:" header lands ABOVE the bar so the args block's tail and
  // the bar don't read as one chunk (both can end in a "+N more lines" marker).
  test('output: header renders above the | bar on success', () => {
    setVerboseConfig({ filters: ['all'] });
    const out = stripAnsi(renderMessageToText(toolMsg(), 'kiro_default'));
    const headerIdx = out.indexOf('output:');
    const firstBarIdx = out.indexOf('│');
    expect(headerIdx).toBeGreaterThan(-1);
    expect(firstBarIdx).toBeGreaterThan(headerIdx);
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
    ...DEFAULT_DISPLAY,
    subagent: { ...DEFAULT_DISPLAY.subagent },
    outputMaxLines: null,
    argsMaxChars: 80,
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

  it.each<{ name: string; cap: number | null; count: number }>([
    {
      name: 'outputMaxLines=null renders all lines with no marker',
      cap: null,
      count: 8,
    },
    {
      name: 'outputMaxLines does not fire when source fits the cap',
      cap: 50,
      count: 5,
    },
  ])('$name', ({ cap, count }) => {
    setDisplay({ outputMaxLines: cap });
    const lines = Array.from({ length: count }, (_, i) => `out-${i}`).join(
      '\n'
    );
    const out = stripAnsi(
      renderMessageToText(buildToolMsg(lines), 'kiro_default')
    );
    expect(out).toContain('out-0');
    expect(out).toContain(`out-${count - 1}`);
    expect(out).not.toMatch(/truncated/);
  });

  test('cap counts logical (source) lines — long lines no longer multiply against the cap', () => {
    setDisplay({ outputMaxLines: 3 });
    // Each 200-char source line wraps to several visual rows at termCols=40, so
    // cap=3 fires hard: counting logical lines keeps the tail window meaningful.
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
    // Block-mode honors the per-value char cap (inline mode already did).
    setDisplay({ argsMaxChars: 10 });
    const content = JSON.stringify({
      command: 'this-is-a-pretty-long-shell-command --with --flags',
      path: 'a/very/long/path/to/some/deeply/nested/file.ts',
      // Explicit purpose so the tool-name line shows reasoning rather than
      // falling back to args.command (which would then carry pre-clip text
      // for an unrelated reason).
      // Explicit purpose so the tool-name line shows reasoning, not args.command.
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
    // Assert under the args header only — the reasoning line still names the tool.
    const argsBlock = out.split('demo block-mode arg clipping')[1] ?? '';
    expect(argsBlock).not.toContain('pretty-long-shell-command');
    expect(argsBlock).not.toContain('deeply/nested/file.ts');
  });

  it.each<{
    name: string;
    display: Partial<VerboseDisplayConfig>;
    content: Record<string, unknown>;
    contains: string[];
  }>([
    {
      name: 'argsMaxChars=null leaves long string values intact',
      display: { argsMaxChars: null },
      content: { command: 'echo hello-world-from-the-other-side' },
      contains: ['hello-world-from-the-other-side'],
    },
    {
      name: 'argsMaxLines=null leaves the args block untouched',
      display: { argsMaxLines: null },
      content: { a: 'one', b: 'two', c: 'three' },
      contains: ['a: one', 'b: two', 'c: three'],
    },
  ])('$name', ({ display, content, contains }) => {
    setDisplay(display);
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-args-nocap',
          role: 'tool_use',
          name: 'shell',
          content: JSON.stringify(content),
          isFinished: true,
          result: { status: 'success', output: 'ok' },
        },
        'kiro_default'
      )
    );
    for (const s of contains) expect(out).toContain(s);
    expect(out).not.toMatch(/truncated/);
  });

  test('argsMaxLines=null + multi-line value: NO per-value truncation marker', () => {
    // P438130055: "unlimited" (argsMaxLines=null) must also lift the per-value
    // 5-line clamp inside formatArgLines, else multi-line args (heredocs,
    // scripts, patches) still got clipped. argsMaxChars=null too so the
    // per-line char cap can't masquerade as the bug.
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
    // Follow-up to P438130055: block mode must NOT keep a per-value cap when
    // argsMaxLines is finite, else its inner "(+N more lines)" marker gets
    // chopped by applyLineCap as one dropped row and the block marker reports
    // "+1" while dozens hide. Block-level applyLineCap is the single source of
    // truth. argsMaxLines=5 + 32-line value → 5 visible, 27 hidden, 32 total.
    setDisplay({ argsMaxLines: 5, argsMaxChars: null });
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
    // Block-level marker reports 27 hidden — not the pre-fix misleading "+1".
    expect(out).toMatch(/\.\.\. \(truncated; \+27 more lines\)/);
    expect(out).not.toMatch(/\(truncated; \+1 more lines\)/);
    // Guard the dropped per-value marker doesn't reappear in the visible portion.
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
    // Regression: a multi-MB single line (e.g. grep matching a minified bundle)
    // made formatBarBlock allocate a cell object per code point → OOM, since
    // applyTailLineCap only runs after it returns. Fix: clip each input line to
    // MAX_INPUT_LINE_CHARS (200_000), keeping the TAIL so it composes with the
    // downstream tail-keep cap.
    setDisplay({ outputMaxLines: null }); // unbounded: prove it doesn't OOM regardless
    const huge = 'x'.repeat(1_000_000);
    const out = stripAnsi(
      renderMessageToText(buildToolMsg(huge), 'kiro_default', {
        termCols: 80,
      })
    );
    // Clip marker is emitted before the wrapped tail. 1_000_000 -
    // 200_000 = 800_000 chars hidden.
    expect(out).toMatch(/\.\.\. \(line clipped; \+800000 chars before\)/);
    // Surviving bar rows are bounded (~2500 for 200K chars at 80 cols).
    // Tripwire against silent regression toward O(input size) memory growth.
    const barLines = out.split('\n').filter((l) => l.includes('│'));
    expect(barLines.length).toBeLessThan(3000);
    // Tail is preserved — last char of the input is the last char of the
    // last bar row (modulo the trailing tool-call newline).
    const lastBar = barLines[barLines.length - 1] ?? '';
    expect(lastBar.endsWith('x')).toBe(true);
  }, 30_000);

  test('lines under MAX_INPUT_LINE_CHARS are not clipped', () => {
    // Companion to the OOM test: a long-but-plausible 50K single line (JSON
    // blob, stack trace) renders without a "(line clipped)" marker, so a
    // future tightening of MAX_INPUT_LINE_CHARS can't clip legit output.
    setDisplay({ outputMaxLines: null });
    const long = 'y'.repeat(50_000);
    const out = stripAnsi(
      renderMessageToText(buildToolMsg(long), 'kiro_default', {
        termCols: 80,
      })
    );
    expect(out).not.toContain('(line clipped');
  });

  // Write diffs OPT OUT of outputMaxLines: the whole change is the payload the
  // user reviews, so the diff body renders in full whether a cap is set or null
  // (read bodies still honor the cap). See d26327c03.
  it.each<{ name: string; cap: number | null; count: number }>([
    {
      name: 'outputMaxLines=5 does not cap fs_write create diff',
      cap: 5,
      count: 30,
    },
    {
      name: 'outputMaxLines=null leaves write diff uncapped',
      cap: null,
      count: 20,
    },
  ])('$name', ({ cap, count }) => {
    setDisplay({ outputMaxLines: cap });
    const content = Array.from({ length: count }, (_, i) => `line-${i}`).join(
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
            content,
          }),
          isFinished: true,
          result: {
            status: 'success',
            output: `Successfully created src/big.ts (${count} lines).`,
          },
        } as any,
        'kiro_default'
      )
    );
    expect(out).toContain('fs_write');
    // Every line survives — head and tail — and no truncation marker appears.
    expect(out).toMatch(/\+\s+line-0/);
    expect(out).toMatch(new RegExp(`\\+\\s+line-${count - 1}`));
    expect(out).not.toMatch(/\.\.\. \(truncated; \+\d+ more lines\)/);
  });

  // The `Successfully created/replaced ...` line duplicates the diff, so it's
  // dropped from scrollback ("write diff IS the write output"). Errors still
  // surface (next test pins that).
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
    // Diff body present; success chrome dropped even with filters:['all'].
    expect(out).toMatch(/\+\s+hello/);
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

  // 10 top-level keys → 10 rows pre-cap; outputMaxLines=3 keeps a tail window.
  const TEN_KEYS: Record<string, number> = {};
  for (let i = 0; i < 10; i++) TEN_KEYS[`k${i}`] = i;

  // Sage-green tint (#a3c0a3) = "successful result" signal. Both text-shape and
  // JSON-shape success outputs must carry it (same truecolor SGR open code).
  const GREEN_SGR = '\x1b[38;2;163;192;163m';

  it.each<{
    name: string;
    output?: unknown;
    msgOverride?: Record<string, unknown>;
    display?: Partial<VerboseDisplayConfig>;
    contains?: string[];
    absent?: string[];
    matches?: RegExp[];
    notMatches?: RegExp[];
    rawContains?: string[]; // assert against un-stripped output (SGR checks)
  }>([
    {
      // Plain string output stays a flat bar block (text path unchanged).
      name: 'plain string output renders as a flat bar block, no key: shape',
      msgOverride: {
        id: 't-text-1',
        name: 'execute_bash',
        content: JSON.stringify({ command: 'echo hi' }),
        result: { status: 'success', output: 'line-a\nline-b' },
      },
      contains: ['│ line-a', '│ line-b'],
      notMatches: [/key:/],
    },
    {
      // No items/content/shell keys → key:value tree, never a raw brace dump.
      name: 'unknown json envelope renders as key:value tree under the bar',
      output: { status: 'ok', count: 3, query: 'find me a thing' },
      contains: ['│ status: ok', '│ count: 3', '│ query: find me a thing'],
      absent: ['{"status":"ok"'],
    },
    {
      // Real MCP shape { items: [{ Json: {...} }] } with no known string keys.
      name: 'items[].Json envelope pretty-prints the inner object',
      output: { items: [{ Json: { matches: 5, latency_ms: 42 } }] },
      contains: ['│ matches: 5', '│ latency_ms: 42'],
    },
    {
      // Args path caps at maxDepth=4; output path must not (user caps via lines).
      name: 'deeply nested json (>4 levels) does NOT collapse to safeJson',
      output: { a: { b: { c: { d: { e: { f: 'deep-value' } } } } } },
      contains: ['f: deep-value'],
      notMatches: [/\{"f":"deep-value"\}/],
    },
    {
      // Tail-window cap mirrors text output (both funnel through applyTailLineCap).
      name: 'outputMaxLines applies to the json tree like it does to text',
      output: TEN_KEYS,
      display: { outputMaxLines: 3, argsMaxChars: 80 },
      contains: ['k7: 7', 'k9: 9'],
      absent: ['k0: 0', 'k6: 6'],
      matches: [/\(truncated; \+7 more lines above\)/],
    },
    {
      // Explicit error: red error bar, structured output must not leak.
      name: 'error path on a json envelope renders the error text only',
      msgOverride: {
        id: 't-err-1',
        content: JSON.stringify({ query: 'q' }),
        result: {
          status: 'error',
          error: 'something went wrong',
          output: { items: [{ Json: { ignored: true } }] },
        },
      },
      contains: ['│ something went wrong'],
      absent: ['ignored: true'],
    },
    {
      // JSON-shape success bodies pick up the same green tint as text-shape.
      // Keep raw output — stripping ANSI would erase the SGR we assert.
      name: 'json-shape output bodies carry the success green tint',
      output: { status: 'ok', count: 3 },
      rawContains: [GREEN_SGR],
    },
    {
      // Apples-to-apples: the text-shape path carries the same tint.
      name: 'text-shape output bodies carry the success green tint',
      msgOverride: {
        id: 't-text-tint',
        name: 'execute_bash',
        content: JSON.stringify({ command: 'echo hi' }),
        result: { status: 'success', output: 'hello' },
      },
      rawContains: [GREEN_SGR],
    },
  ])(
    '$name',
    ({
      output,
      msgOverride,
      display,
      contains,
      absent,
      matches,
      notMatches,
      rawContains,
    }) => {
      if (display) {
        setVerboseConfig({
          display: {
            ...DEFAULT_DISPLAY,
            subagent: { ...DEFAULT_DISPLAY.subagent },
            ...display,
          },
        });
      }
      const msg = msgOverride
        ? { ...buildJsonOutputMsg(undefined), ...msgOverride }
        : buildJsonOutputMsg(output);
      const raw = renderMessageToText(msg, 'kiro_default');
      const out = stripAnsi(raw);
      for (const s of contains ?? []) expect(out).toContain(s);
      for (const s of absent ?? []) expect(out).not.toContain(s);
      for (const re of matches ?? []) expect(out).toMatch(re);
      for (const re of notMatches ?? []) expect(out).not.toMatch(re);
      for (const s of rawContains ?? []) expect(raw).toContain(s);
    }
  );
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
    ...DEFAULT_DISPLAY,
    subagent: { ...DEFAULT_DISPLAY.subagent },
    outputMaxLines: null,
    argsMaxChars: 80,
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
        ...DEFAULT_DISPLAY,
        subagent: { ...DEFAULT_DISPLAY.subagent },
        showToolReasoning: false,
        toolArgsMode: 'inline',
        showElapsed: false,
        outputMaxLines: null,
        argsMaxChars: 200,
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
