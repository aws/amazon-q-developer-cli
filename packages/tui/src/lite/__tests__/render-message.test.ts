import { describe, test, expect, beforeEach } from 'vitest';
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

  test('todo_list create renders header as `tasks create` plus body', () => {
    const content = JSON.stringify({
      command: 'create',
      task_list_description: 'fix the renderer',
      tasks: [
        { task_description: 'first thing' },
        { task_description: 'second thing' },
      ],
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 'tt1',
          role: 'tool_use',
          name: 'todo_list',
          content,
          isFinished: true,
          startTime: 0,
          finishTime: 250,
        },
        'kiro_default'
      )
    );
    const lines = out.split('\n');
    // Display name is `tasks` (not `todo_list`), with `create` as inline arg.
    expect(lines[0]).toContain('tasks create');
    expect(lines[0]).not.toContain('todo_list');
    expect(out).toContain('first thing');
    expect(out).toContain('second thing');
    expect(out).toContain('fix the renderer');
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

  test('toolArgsMode=off suppresses body but keeps the header', () => {
    setVerboseConfig({ display: { ...DEFAULT_DISPLAY, toolArgsMode: 'off' } });
    const content = JSON.stringify({
      command: 'create',
      task_list_description: 'd',
      tasks: [{ task_description: 'only task' }],
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 'tt-off',
          role: 'tool_use',
          name: 'todo_list',
          content,
          isFinished: true,
        },
        'kiro_default'
      )
    );
    const lines = out.split('\n');
    // Header is still there.
    expect(lines[0]).toContain('tasks create');
    // Body is suppressed.
    expect(out).not.toContain('only task');
  });

  test('argsMaxLines clamps the body with a truncation marker', () => {
    setVerboseConfig({
      display: { ...DEFAULT_DISPLAY, argsMaxLines: 3 },
    });
    const content = JSON.stringify({
      command: 'create',
      task_list_description: 'list with many tasks',
      tasks: Array.from({ length: 10 }, (_, i) => ({
        task_description: `task ${i + 1}`,
      })),
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 'tt-cap',
          role: 'tool_use',
          name: 'todo_list',
          content,
          isFinished: true,
        },
        'kiro_default'
      )
    );
    expect(out).toMatch(/\.\.\. \(truncated; \+\d+ more lines\)/);
    // First couple of tasks visible.
    expect(out).toContain('task 1');
    // Later tasks dropped.
    expect(out).not.toContain('task 10');
  });

  test('malformed args fall through to generic JSON pretty-printer', () => {
    // When formatTaskToolBody returns null (unknown command), the generic
    // path takes over so the user still sees the args. Locks the fallback.
    const content = JSON.stringify({ command: 'unknown_command', foo: 'bar' });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 'tt-fallback',
          role: 'tool_use',
          name: 'todo_list',
          content,
          isFinished: true,
        },
        'kiro_default'
      )
    );
    // Falls back to the wire name, not 'tasks'.
    expect(out).toContain('todo_list');
    // Generic args printer surfaces the fields.
    expect(out).toContain('foo: bar');
  });

  test('errored tool call surfaces the error body below the structured render', () => {
    // Pre-mortem fix: the write-tool path appends renderVerboseOutput on
    // error so the user sees the actual failure cause. Mirror that here so
    // a TaskStore filesystem error doesn't leave the user blind with just a
    // FAILED chip on the header.
    const content = JSON.stringify({
      command: 'complete',
      completed_task_ids: ['1'],
      context_update: 'tried to complete',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 'tt-error',
          role: 'tool_use',
          name: 'todo_list',
          content,
          isFinished: true,
          result: {
            status: 'error',
            error: 'TaskStore: permission denied writing /tmp/tasks',
          },
        },
        'kiro_default'
      )
    );
    // Header still shows the structured rendering.
    expect(out).toContain('tasks complete');
    expect(out).toContain('FAILED');
    expect(out).toContain('completed:');
    expect(out).toContain('#1');
    // Error message is appended below, not silently swallowed.
    expect(out).toContain('TaskStore: permission denied');
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

  test('truncation:output preview reflects the passed-in output cap', () => {
    // 60-line fixture; cap to 5 — overflow marker should mention 55 dropped.
    const display = { ...FULL_DISPLAY, outputMaxLines: 5 };
    const text = renderVerbosityPreview('truncation:output', display, ['all']);
    const stripped = stripAnsi(text);
    expect(stripped).toContain('+55 more lines');
  });

  test('truncation:output preview with unlimited cap shows no marker', () => {
    const display = { ...FULL_DISPLAY, outputMaxLines: null };
    const text = renderVerbosityPreview('truncation:output', display, ['all']);
    const stripped = stripAnsi(text);
    expect(stripped).not.toContain('more lines)');
  });

  test('output preview honors the filter override (no bar when filters empty)', () => {
    // With filters: [] the output bars are gated off, so the rendered
    // preview should not include the `│ ` output bar that appears under
    // each tool when filters allow it.
    const text = renderVerbosityPreview('output', FULL_DISPLAY, []);
    const stripped = stripAnsi(text);
    expect(stripped).not.toContain('│ On branch feature');
  });

  test('output preview with all filter shows the output bar', () => {
    const text = renderVerbosityPreview('output', FULL_DISPLAY, ['all']);
    const stripped = stripAnsi(text);
    expect(stripped).toContain('│ On branch feature');
  });

  test('truncation:output preview widens filters so the bar always renders', () => {
    // User has empty filters — in real scrollback nothing would surface.
    // The truncation preview deliberately widens so the cap is visible,
    // and prepends a hint explaining the gap between preview and reality.
    const display = { ...FULL_DISPLAY, outputMaxLines: 5 };
    const text = renderVerbosityPreview('truncation:output', display, []);
    const stripped = stripAnsi(text);
    // Cap fired despite empty saved filters.
    expect(stripped).toContain('+55 more lines');
    // Hint surfaces so the user understands this is preview-only.
    expect(stripped).toContain('preview-only');
  });

  test('truncation:output preview picks a tool from the user enabled categories', () => {
    // User has only `read` enabled — the fixture should use fs_read so the
    // demo matches a tool they'd actually see in real scrollback.
    const display = { ...FULL_DISPLAY, outputMaxLines: 5 };
    const text = renderVerbosityPreview('truncation:output', display, ['read']);
    const stripped = stripAnsi(text);
    // fs_read tool name appears in the preview.
    expect(stripped).toContain('fs_read');
    // No "preview-only" hint since the user's filters already cover the
    // chosen tool — the demo matches their real scrollback.
    expect(stripped).not.toContain('preview-only');
  });

  test('truncation:output preview suppresses hint when filters already cover the tool', () => {
    // User has shell enabled — fixture picks shell and renders without hint.
    const display = { ...FULL_DISPLAY, outputMaxLines: 5 };
    const text = renderVerbosityPreview('truncation:output', display, [
      'shell',
    ]);
    const stripped = stripAnsi(text);
    expect(stripped).not.toContain('preview-only');
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
