import { describe, test, it, expect, beforeEach } from 'vitest';
import chalk from 'chalk';
import {
  renderSystemError,
  renderSystemInfo,
  renderTurnSummary,
  renderMessageToText,
  renderVerbosityPreview,
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

// Force chalk colors for consistent test output
chalk.level = 3;

describe('renderMessageToText with shellOutput', () => {
  test('routes Model messages with shellOutput=true to the gutter formatter', () => {
    // Same body the /tmp tail of a `!read -p` would produce: bash's
    // line-by-line output, no agent prose. The dispatch in
    // renderMessageToText should bypass the `Kiro:` tag entirely.
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 'm1',
          role: 'model',
          content: 'Enter PIN:\nGot: 1234',
          shellOutput: true,
        },
        'Kiro'
      )
    );
    expect(out).not.toContain('Kiro:');
    expect(out.split('\n')).toEqual(['! Enter PIN:', '! Got: 1234']);
  });

  test('does not run markdown rendering on shell output', () => {
    // Without the shellOutput branch, the Model case calls
    // renderAgentMessage which forces the body through the markdown
    // pipeline. That mangles output containing `*` (glob expansions,
    // ascii box drawings), `_` (filenames), `#` (heading-like comments),
    // backticks (shell quoting). The branch is what keeps shell output
    // round-trip-safe.
    const tricky = '*.ts files: src/_main.ts (# 1)';
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 'm1',
          role: 'model',
          content: tricky,
          shellOutput: true,
        },
        'Kiro'
      )
    );
    expect(out).toBe('! ' + tricky);
  });

  test('shellOutput Model with empty content renders nothing', () => {
    // The store seeds an empty Model row at command-spawn time so the
    // PTY's onData chunk handler has a stable id to update. We need to
    // tolerate that empty row without falling back to the agent path
    // (which would render an empty `Kiro:` line).
    const out = renderMessageToText(
      {
        id: 'm1',
        role: 'model',
        content: '',
        shellOutput: true,
      },
      'Kiro'
    );
    expect(out).toBe('');
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
    const result = renderTurnSummary(input);
    for (const c of contains) expect(result).toContain(c);
    for (const a of absent ?? []) expect(result).not.toContain(a);
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
      // formatToolArgs, leaving no visible args after running.
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
      const out = stripAnsi(
        renderMessageToText(
          {
            id: `t-stream-${_name}`,
            role: 'tool_use',
            name: 'fs_write',
            content: JSON.stringify(args),
            isFinished: false,
          },
          'kiro_default'
        )
      );
      expect(out).not.toContain('undefined');
      for (const a of extraAbsent) expect(out).not.toContain(a);
      expect(out).toContain('fs_write');
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
      const out = stripAnsi(
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
        )
      );
      expect(out).toContain('DENIED');
      for (const p of present) expect(out).toMatch(p);
      for (const a of absent) expect(out).not.toMatch(a);
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
    for (const s of c.contains ?? []) expect(out).toContain(s);
    for (const s of c.absent ?? []) expect(out).not.toContain(s);
    for (const re of c.matches ?? []) expect(out).toMatch(re);
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

describe('renderVerbosityPreview', () => {
  // Default-display fixture matches normal density (everything on, no caps).
  const FULL_DISPLAY = {
    showToolReasoning: true,
    toolArgsMode: 'block' as const,
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
    showWriteDiffs: true,
    argsMaxLines: null,
    outputMaxLines: null,
    argsMaxChars: 80,
    outputMaxChars: null,
  };

  test('every preview key renders non-empty output', () => {
    const keys = [
      'top',
      'density',
      'tool',
      'subagent',
      'output',
      'truncation:args',
      'truncation:output',
    ] as const;
    for (const key of keys) {
      const text = renderVerbosityPreview(key, FULL_DISPLAY, ['all']);
      const stripped = stripAnsi(text);
      expect(stripped.length).toBeGreaterThan(0);
    }
  });

  // truncation:output preview: a 60-line fixture is capped at outputMaxLines.
  // The preview widens empty filters so the cap is always visible (prepending a
  // "preview-only" hint), but suppresses that hint when the user's filters
  // already cover the chosen tool. Fixture tool follows enabled categories.
  test.each<{
    name: string;
    cap: number | null;
    filters: string[];
    contains?: string[];
    absent?: string[];
  }>([
    {
      name: 'reflects the passed-in output cap (55 of 60 dropped)',
      cap: 5,
      filters: ['all'],
      contains: ['+55 more lines'],
    },
    {
      name: 'unlimited cap shows no marker',
      cap: null,
      filters: ['all'],
      absent: ['more lines)'],
    },
    {
      name: 'widens empty filters so the bar renders, with a preview-only hint',
      cap: 5,
      filters: [],
      contains: ['+55 more lines', 'preview-only'],
    },
    {
      name: 'picks fs_read when only read is enabled; no hint (matches reality)',
      cap: 5,
      filters: ['read'],
      contains: ['fs_read'],
      absent: ['preview-only'],
    },
    {
      name: 'suppresses hint when filters already cover the tool (shell)',
      cap: 5,
      filters: ['shell'],
      absent: ['preview-only'],
    },
  ])(
    'truncation:output preview $name',
    ({ cap, filters, contains, absent }) => {
      const display = { ...FULL_DISPLAY, outputMaxLines: cap };
      const stripped = stripAnsi(
        renderVerbosityPreview('truncation:output', display, filters)
      );
      for (const c of contains ?? []) expect(stripped).toContain(c);
      for (const a of absent ?? []) expect(stripped).not.toContain(a);
    }
  );

  // `│ ` output bars are gated by the filter list: empty filters suppress them,
  // ['all'] surfaces them under each tool.
  it.each<{ name: string; filters: string[]; present: boolean }>([
    {
      name: 'honors the filter override (no bar when filters empty)',
      filters: [],
      present: false,
    },
    {
      name: 'with all filter shows the output bar',
      filters: ['all'],
      present: true,
    },
  ])('output preview $name', ({ filters, present }) => {
    const stripped = stripAnsi(
      renderVerbosityPreview('output', FULL_DISPLAY, filters)
    );
    if (present) expect(stripped).toContain('│ On branch feature');
    else expect(stripped).not.toContain('│ On branch feature');
  });

  test('top preview includes a richer fixture mix (write + grep + mcp + agent)', () => {
    // Diversified fixtures so users see what every kind of tool looks like
    // under their settings, plus an agent message between tool calls.
    const text = renderVerbosityPreview('top', FULL_DISPLAY, ['all'], {
      expanded: true,
    });
    const stripped = stripAnsi(text);
    // Write tool (with its diff path).
    expect(stripped).toContain('fs_write');
    // Grep with the long pattern.
    expect(stripped).toContain('grep');
    // MCP tool (the recall fixture).
    expect(stripped).toContain('mcp__nova-memory-mcp__recall');
    // Agent message body — ensures non-tool content also renders.
    expect(stripped).toContain('Found four call sites');
  });
});
