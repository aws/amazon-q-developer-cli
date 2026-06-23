import { describe, test, it, expect, beforeEach } from 'vitest';
import chalk from 'chalk';
import {
  renderSystemError,
  renderSystemInfo,
  renderTurnSummary,
  renderMessageToText,
} from '../render.js';
import {
  setVerboseConfig,
  resetVerboseCache,
  DEFAULT_DISPLAY,
  type VerboseDisplayConfig,
} from '../verbose.js';
import stripAnsi from 'strip-ansi';
import { useTempKiroHome } from './temp-kiro-home.js';
import { expectRender } from './expect-render.js';

useTempKiroHome();

// Force chalk colors for consistent test output
chalk.level = 3;

describe('renderMessageToText with shellOutput', () => {
  // shellOutput=true Model rows bypass the `Kiro:` tag AND the markdown
  // pipeline (which would mangle `*` globs, `_` filenames, `#` comments,
  // backticks) — each body line gets a `! ` gutter prefix and round-trips
  // verbatim. Empty content (the store's spawn-time placeholder row) must
  // render nothing, not an empty `Kiro:` line.
  const tricky = '*.ts files: src/_main.ts (# 1)';
  test.each<{ name: string; content: string; expected: string }>([
    {
      name: 'gutter-prefixes each line, no Kiro: tag',
      content: 'Enter PIN:\nGot: 1234',
      expected: '! Enter PIN:\n! Got: 1234',
    },
    {
      name: 'does not run markdown rendering (markers round-trip)',
      content: tricky,
      expected: '! ' + tricky,
    },
    { name: 'empty content renders nothing', content: '', expected: '' },
  ])('$name', ({ content, expected }) => {
    const out = stripAnsi(
      renderMessageToText(
        { id: 'm1', role: 'model', content, shellOutput: true },
        'Kiro'
      )
    );
    expect(out).toBe(expected);
  });
});

describe('renderSystemError', () => {
  test('shows error prefix in red', () => {
    const result = renderSystemError('Something failed');
    expect(result).toContain('error:');
    expect(result).toContain('Something failed');
  });
});

describe('renderSystemInfo', () => {
  test('shows message dimmed', () => {
    const result = renderSystemInfo('Loading...');
    expect(result).toContain('Loading...');
  });
});

describe('renderTurnSummary', () => {
  test.each<{
    name: string;
    input: Parameters<typeof renderTurnSummary>[0];
    contains: string[];
    absent?: string[];
  }>([
    {
      name: 'renders metering usage',
      input: {
        meteringUsage: [
          { value: 1234, unit: 'token', unitPlural: 'tokens' },
          { value: 567, unit: 'token', unitPlural: 'tokens' },
        ],
      },
      contains: ['1234 tokens', '567 tokens'],
    },
    {
      name: 'renders duration when provided',
      input: {
        meteringUsage: [{ value: 100, unit: 'token', unitPlural: 'tokens' }],
        durationMs: 3200,
      },
      contains: ['3s'],
    },
    {
      name: 'singular unit for value=1',
      input: {
        meteringUsage: [{ value: 1, unit: 'request', unitPlural: 'requests' }],
      },
      contains: ['1 request'],
      absent: ['1 requests'],
    },
  ])('$name', ({ input, contains, absent }) => {
    expectRender(renderTurnSummary(input), { contains, absent });
  });
});

describe('renderMessageToText (tool_use)', () => {
  // These tests rely on the implicit DEFAULT_DISPLAY (block args, reasoning
  // on). Other suites in this file write custom configs to disk and leak
  // their cache; resetting at the start of each test guarantees a clean
  // baseline so block-mode rendering shows reasoning + args, not a chip.
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: [] });
  });

  // First-line reasoning vs args-block rendering for finished tool calls.
  // Reasoning (__tool_use_purpose) is preferred over args.command on the header;
  // when absent, the header stays bare (we don't synthesize from args.command /
  // path) and the value appears exactly once in the args block below. `headHas`
  // / `headLacks` assert on line[0]; `once` pins a value appears on exactly one
  // line (the duplication-bug guard).
  test.each<{
    name: string;
    tool: string;
    content: Record<string, unknown>;
    headHas?: string[];
    headLacks?: string[];
    contains?: string[];
    once?: string[];
    notMatch?: RegExp[];
    noPurposeRow?: boolean;
  }>([
    {
      name: 'shell tool shows reasoning on first line, args below',
      tool: 'shell',
      content: {
        command: 'git log --oneline -5',
        working_dir: '/tmp/repo',
        __tool_use_purpose: 'Exercise the Shell tool UI with git log',
      },
      headHas: ['shell', 'Exercise the Shell tool UI with git log'],
      headLacks: ['git log --oneline -5'],
      contains: ['command: git log --oneline -5', 'working_dir: /tmp/repo'],
      noPurposeRow: true,
    },
    {
      // Single short-string args (recall {query}) used to be skipped by
      // formatToolArgLines, leaving no visible args after running.
      name: 'recall-style single short arg still shows in scrollback',
      tool: 'recall',
      content: {
        query: 'lite TUI tool rendering',
        __tool_use_purpose: 'check memory for prior context',
      },
      contains: [
        'check memory for prior context',
        'query: lite TUI tool rendering',
      ],
    },
    {
      // MCP nested args (remember messages[]) pretty-print with indented keys,
      // not a single-line JSON blob.
      name: 'nested array of objects is pretty-printed with indentation',
      tool: 'remember',
      content: {
        messages: [
          { role: 'USER', content: 'use some tools test stuff' },
          { role: 'ASSISTANT', content: 'Ran a bunch of tools.' },
        ],
        __tool_use_purpose: 'persist conversation',
      },
      contains: [
        'messages:',
        '-',
        'role: USER',
        'content: use some tools test stuff',
        'role: ASSISTANT',
      ],
      notMatch: [/messages:\s*\[\{"role"/],
    },
    {
      // No __tool_use_purpose → empty reasoning slot (no synthesis from
      // args.command); args block still shows the command exactly once.
      name: 'no inline reasoning when agent omits __tool_use_purpose (block mode)',
      tool: 'shell',
      content: { command: 'ls -la' },
      headHas: ['shell'],
      headLacks: ['ls -la'],
      contains: ['command: ls -la'],
      once: ['ls -la'],
    },
    {
      // Duplication-bug repro: the header reasoning slot used to fall back to
      // args.path (purple) while the args block printed it again (white).
      name: 'block mode without __tool_use_purpose: path appears exactly once',
      tool: 'fs_read',
      content: { operations: [{ path: '/etc/hosts', limit: 50 }] },
      headHas: ['fs_read'],
      headLacks: ['/etc/hosts'],
      once: ['/etc/hosts'],
    },
  ])(
    '$name',
    ({
      tool,
      content,
      headHas,
      headLacks,
      contains,
      once,
      notMatch,
      noPurposeRow,
    }) => {
      const out = stripAnsi(
        renderMessageToText(
          {
            id: `t-${tool}`,
            role: 'tool_use',
            name: tool,
            content: JSON.stringify(content),
            isFinished: true,
          },
          'kiro_default'
        )
      );
      const lines = out.split('\n');
      for (const h of headHas ?? []) expect(lines[0]).toContain(h);
      for (const h of headLacks ?? []) expect(lines[0]).not.toContain(h);
      for (const c of contains ?? []) expect(out).toContain(c);
      for (const o of once ?? [])
        expect(lines.filter((l) => l.includes(o))).toHaveLength(1);
      for (const re of notMatch ?? []) expect(out).not.toMatch(re);
      if (noPurposeRow)
        expect(
          lines.filter((l) => l.includes('__tool_use_purpose'))
        ).toHaveLength(0);
    }
  );

  // Streaming write-tool guard. Mid-stream the JSON can land with `command`
  // set but `path`/`content` not yet arrived and no `__tool_use_purpose` — the
  // prior template interpolated `undefined`/`(0 lines)` next to the tool name.
  // Suppress the summary until there's a real label; the bare tool-call line is
  // the correct "loading" appearance. (Header summary removed in 7bc885a9a.)
  test.each([
    ['no path/purpose, create', { command: 'create' }, ['(0 lines)']],
    ['no path/purpose, insert', { command: 'insert' }, ['+0 lines']],
    ['path but no content', { command: 'create', path: 'src/foo.ts' }, []],
  ])(
    'streaming fs_write (%s): no undefined / line-count noise',
    (_name, args, extraAbsent) => {
      expectRender(
        renderMessageToText(
          {
            id: `t-stream-${_name}`,
            role: 'tool_use',
            name: 'fs_write',
            content: JSON.stringify(args),
            isFinished: false,
          },
          'kiro_default'
        ),
        { contains: ['fs_write'], absent: ['undefined', ...extraAbsent] }
      );
    }
  );

  // Denied write tools must STILL render as a diff (line numbers, +/- gutter) —
  // the same shape shown on the approval prompt — not the generic args tree,
  // which reads worse (a wall of indented `content:` rows for big blobs). The
  // DENIED status surfaces in the header. `present`/`absent` are matched as
  // regexes; absent guards against the args-tree fallback re-appearing.
  test.each([
    [
      'create',
      { command: 'create', path: 'src/foo.ts', content: 'hello\nworld' },
      [/1 \+\s+hello/, /2 \+\s+world/],
      [/^\s*command:\s*create/m, /^\s*content:\s*hello/m],
    ],
    [
      'strReplace',
      {
        command: 'strReplace',
        path: 'src/bar.ts',
        oldStr: 'old line',
        newStr: 'new line',
      },
      [/-\s*old line/, /\+\s*new line/],
      [/^\s*oldStr:\s*old line/m, /^\s*newStr:\s*new line/m],
    ],
  ])(
    'denied fs_write %s renders as a diff, not a raw args tree',
    (_name, args, present, absent) => {
      expectRender(
        renderMessageToText(
          {
            id: `t-denied-${_name}`,
            role: 'tool_use',
            name: 'fs_write',
            content: JSON.stringify(args),
            isFinished: true,
            status: 'rejected',
          } as any,
          'kiro_default'
        ),
        { contains: ['DENIED'], matches: present, notMatches: absent }
      );
    }
  );
});

describe('renderMessageToText for task tools', () => {
  beforeEach(() => {
    resetVerboseCache();
    setVerboseConfig({ filters: [] });
  });

  // Task-tool (todo_list) message rendering: the header rewrites the wire
  // name → `tasks` with the command as the inline chip, then a structured
  // body (gated by toolArgsMode / argsMaxLines). Cases share the same
  // tool_use message shell and only vary content / verbose config / result.
  const renderTask = (
    content: Record<string, unknown>,
    extra: Partial<{
      result: { status: string; error?: string };
      startTime: number;
      finishTime: number;
    }> = {}
  ) =>
    stripAnsi(
      renderMessageToText(
        {
          id: 'tt',
          role: 'tool_use',
          name: 'todo_list',
          content: JSON.stringify(content),
          isFinished: true,
          ...extra,
        },
        'kiro_default'
      )
    );
  it.each<{
    name: string;
    content: Record<string, unknown>;
    display?: Partial<VerboseDisplayConfig>;
    extra?: Parameters<typeof renderTask>[1];
    headerContains?: string[];
    headerAbsent?: string[];
    contains?: string[];
    absent?: string[];
    matches?: RegExp[];
  }>([
    {
      // Display name is `tasks` (not `todo_list`), `create` as inline arg.
      name: 'create renders header as `tasks create` plus body',
      content: {
        command: 'create',
        task_list_description: 'fix the renderer',
        tasks: [
          { task_description: 'first thing' },
          { task_description: 'second thing' },
        ],
      },
      extra: { startTime: 0, finishTime: 250 },
      headerContains: ['tasks create'],
      headerAbsent: ['todo_list'],
      contains: ['first thing', 'second thing', 'fix the renderer'],
    },
    {
      name: 'toolArgsMode=off suppresses body but keeps the header',
      content: {
        command: 'create',
        task_list_description: 'd',
        tasks: [{ task_description: 'only task' }],
      },
      display: { toolArgsMode: 'off' },
      headerContains: ['tasks create'],
      absent: ['only task'],
    },
    {
      name: 'argsMaxLines clamps the body with a truncation marker',
      content: {
        command: 'create',
        task_list_description: 'list with many tasks',
        tasks: Array.from({ length: 10 }, (_, i) => ({
          task_description: `task ${i + 1}`,
        })),
      },
      display: { argsMaxLines: 3 },
      contains: ['task 1'],
      absent: ['task 10'],
      matches: [/\.\.\. \(truncated; \+\d+ more lines\)/],
    },
    {
      // Unknown command → formatTaskToolBody returns null and the generic
      // JSON printer takes over (wire name kept, fields surfaced).
      name: 'malformed args fall through to generic JSON pretty-printer',
      content: { command: 'unknown_command', foo: 'bar' },
      contains: ['todo_list', 'foo: bar'],
    },
    {
      // On error, renderVerboseOutput appends the failure cause below the
      // structured render (mirrors the write-tool path) so a TaskStore FS
      // error isn't hidden behind a bare FAILED chip.
      name: 'errored tool call surfaces the error body below the structured render',
      content: {
        command: 'complete',
        completed_task_ids: ['1'],
        context_update: 'tried to complete',
      },
      extra: {
        result: {
          status: 'error',
          error: 'TaskStore: permission denied writing /tmp/tasks',
        },
      },
      contains: [
        'tasks complete',
        'FAILED',
        'completed:',
        '#1',
        'TaskStore: permission denied',
      ],
    },
  ])('todo_list $name', (c) => {
    if (c.display) {
      setVerboseConfig({ display: { ...DEFAULT_DISPLAY, ...c.display } });
    }
    const out = renderTask(c.content, c.extra);
    const header = out.split('\n')[0] ?? '';
    for (const s of c.headerContains ?? []) expect(header).toContain(s);
    for (const s of c.headerAbsent ?? []) expect(header).not.toContain(s);
    expectRender(out, {
      contains: c.contains,
      absent: c.absent,
      matches: c.matches,
    });
  });

  test('all three wire aliases (task / todo_list / todo) render with `tasks` display name', () => {
    const content = JSON.stringify({ command: 'list' });
    for (const wireName of ['task', 'todo_list', 'todo']) {
      const out = stripAnsi(
        renderMessageToText(
          {
            id: `t-${wireName}`,
            role: 'tool_use',
            name: wireName,
            content,
            isFinished: true,
          },
          'kiro_default'
        )
      );
      // Header is the rewritten `tasks` plus the `list` command chip.
      expect(out).toContain('tasks list');
      // The wire alias must NOT survive into the rendered header. We
      // can't substring-test for `task` directly (since `tasks` contains
      // it), so use a word-boundary regex anchored on the rewrite to
      // the standardized display name. `\btask\b` would match the wire
      // alias only — and there's no such bare token in the output.
      const wordRe = new RegExp(`\\b${wireName}\\b`);
      expect(wordRe.test(out)).toBe(false);
    }
  });
});
