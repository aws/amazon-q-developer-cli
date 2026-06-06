import {
  describe,
  test,
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
  test('renders metering usage', () => {
    const result = renderTurnSummary({
      meteringUsage: [
        { value: 1234, unit: 'token', unitPlural: 'tokens' },
        { value: 567, unit: 'token', unitPlural: 'tokens' },
      ],
    });
    expect(result).toContain('1234 tokens');
    expect(result).toContain('567 tokens');
  });

  test('renders duration when provided', () => {
    const result = renderTurnSummary({
      meteringUsage: [{ value: 100, unit: 'token', unitPlural: 'tokens' }],
      durationMs: 3200,
    });
    expect(result).toContain('3s');
  });

  test('singular unit for value=1', () => {
    const result = renderTurnSummary({
      meteringUsage: [{ value: 1, unit: 'request', unitPlural: 'requests' }],
    });
    expect(result).toContain('1 request');
    expect(result).not.toContain('1 requests');
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

  // Reasoning is preferred over args.command for the tool's first-line description.
  // Reproduces the bug where a shell tool was showing the bare command instead of
  // the LLM's __tool_use_purpose after approval.
  test('shell tool shows reasoning on first line, args below', () => {
    const content = JSON.stringify({
      command: 'git log --oneline -5',
      working_dir: '/tmp/repo',
      __tool_use_purpose: 'Exercise the Shell tool UI with git log',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't1',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
          startTime: 0,
          finishTime: 124,
        },
        'kiro_default'
      )
    );
    const lines = out.split('\n');
    expect(lines[0]).toContain('shell');
    expect(lines[0]).toContain('Exercise the Shell tool UI with git log');
    expect(lines[0]).not.toContain('git log --oneline -5');
    expect(out).toContain('command: git log --oneline -5');
    expect(out).toContain('working_dir: /tmp/repo');
    // __tool_use_purpose should not be repeated as an arg row
    const purposeLines = lines.filter((l) => l.includes('__tool_use_purpose'));
    expect(purposeLines).toHaveLength(0);
  });

  // Single short-string args (e.g. recall { query: '...' }) used to be skipped
  // by formatToolArgs, leaving the user with no visible args after running.
  test('recall-style single short arg still shows in scrollback', () => {
    const content = JSON.stringify({
      query: 'lite TUI tool rendering',
      __tool_use_purpose: 'check memory for prior context',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't2',
          role: 'tool_use',
          name: 'recall',
          content,
          isFinished: true,
        },
        'kiro_default'
      )
    );
    expect(out).toContain('check memory for prior context');
    expect(out).toContain('query: lite TUI tool rendering');
  });

  // MCP-style nested args (e.g. nova-memory remember messages array of {role, content})
  // should pretty-print with indented keys, not appear as a JSON blob.
  test('nested array of objects is pretty-printed with indentation', () => {
    const content = JSON.stringify({
      messages: [
        { role: 'USER', content: 'use some tools test stuff' },
        { role: 'ASSISTANT', content: 'Ran a bunch of tools.' },
      ],
      __tool_use_purpose: 'persist conversation',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't3',
          role: 'tool_use',
          name: 'remember',
          content,
          isFinished: true,
        },
        'kiro_default'
      )
    );
    // Pretty-printed: messages: header, then -, role:, content: lines
    expect(out).toContain('messages:');
    expect(out).toContain('-');
    expect(out).toContain('role: USER');
    expect(out).toContain('content: use some tools test stuff');
    expect(out).toContain('role: ASSISTANT');
    // Should NOT be the single-line JSON blob form
    expect(out).not.toMatch(/messages:\s*\[\{"role"/);
  });

  // When the agent omits __tool_use_purpose, the inline reasoning slot
  // stays empty — we no longer synthesize a one-liner from args.command /
  // args.path / etc. The args block below still surfaces those values, so
  // the user isn't blind to what the tool is doing; the difference is that
  // purple now reliably means "the agent gave us actual reasoning" instead
  // of "we made something up from the args".
  test('no inline reasoning when agent omits __tool_use_purpose (block mode)', () => {
    const content = JSON.stringify({ command: 'ls -la' });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't4',
          role: 'tool_use',
          name: 'shell',
          content,
          isFinished: true,
        },
        'kiro_default'
      )
    );
    const lines = out.split('\n');
    // First line is the bare tool name — no `ls -la` next to `shell`.
    expect(lines[0]).toContain('shell');
    expect(lines[0]).not.toContain('ls -la');
    // Args block below still shows the command exactly once.
    expect(out).toContain('command: ls -la');
    const commandRows = lines.filter((l) => l.includes('ls -la'));
    expect(commandRows).toHaveLength(1);
  });

  // Direct repro of the duplication bug: with reasoning enabled and block
  // mode (the default), the first-line reasoning slot used to fall back to
  // args.path and paint it purple, while the args block below printed the
  // exact same path in white. Two `path: foo.ts` rows, one purple, one
  // white. This test pins the new behavior so the duplicate can't reappear.
  test('block mode without __tool_use_purpose: path appears exactly once', () => {
    const content = JSON.stringify({
      operations: [{ path: '/etc/hosts', limit: 50 }],
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't5',
          role: 'tool_use',
          name: 'fs_read',
          content,
          isFinished: true,
        },
        'kiro_default'
      )
    );
    const lines = out.split('\n');
    expect(lines[0]).toContain('fs_read');
    expect(lines[0]).not.toContain('/etc/hosts');
    // Path shows up in the args block below — once, not twice.
    const pathRows = lines.filter((l) => l.includes('/etc/hosts'));
    expect(pathRows).toHaveLength(1);
  });

  // Streaming write-tool guard. While the agent is still emitting the tool
  // call, the JSON can land with `command: 'create'` set but `path` and
  // `content` not yet streamed in. Combined with no `__tool_use_purpose`,
  // the prior render template interpolated `undefined` and surfaced
  // `undefined (0 lines)` next to the tool name — implying the agent had
  // already decided on a 0-line file at no path. Suppress the summary
  // entirely until we have a real label; the bare tool-call line + spinner
  // is the correct "loading" appearance.
  test('streaming fs_write with no path / purpose: no "undefined (0 lines)"', () => {
    const content = JSON.stringify({ command: 'create' });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-stream-create',
          role: 'tool_use',
          name: 'fs_write',
          content,
          isFinished: false,
        },
        'kiro_default'
      )
    );
    expect(out).not.toContain('undefined');
    expect(out).not.toContain('(0 lines)');
    // The bare tool-call line still renders so the user sees the tool is
    // in flight.
    expect(out).toContain('fs_write');
  });

  test('streaming fs_write insert with no path / purpose: no "undefined +0 lines"', () => {
    const content = JSON.stringify({ command: 'insert' });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-stream-insert',
          role: 'tool_use',
          name: 'fs_write',
          content,
          isFinished: false,
        },
        'kiro_default'
      )
    );
    expect(out).not.toContain('undefined');
    expect(out).not.toContain('+0 lines');
    expect(out).toContain('fs_write');
  });

  // Streaming write with a path but no content yet: the synthesized
  // "{path} (N lines)" header summary was removed (7bc885a9a), so this
  // transient state renders a bare `fs_write` header — the point of the
  // test is that there's no `undefined` / `(0 lines)` noise in it.
  test('streaming fs_write with path but no content: renders a clean header', () => {
    const content = JSON.stringify({
      command: 'create',
      path: 'src/foo.ts',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-stream-path',
          role: 'tool_use',
          name: 'fs_write',
          content,
          isFinished: false,
        },
        'kiro_default'
      )
    );
    expect(out).not.toContain('undefined');
    expect(out).toContain('fs_write');
  });

  // Denied write tools must STILL render as a diff (with line numbers, +/-
  // gutter) — same shape the user just saw on the approval prompt. Falling
  // back to the generic args tree (`command:`, `path:`, `content:` rows)
  // for the post-deny scrollback row is jarring and reads worse than the
  // diff form, especially for multi-hundred-line content blobs that turn
  // into a wall of indented `# line 1\n# line 2\n...` rows. The DENIED
  // status surfaces in the header line via info.rejected.
  test('denied fs_write create renders as a diff, not a raw args tree', () => {
    const content = JSON.stringify({
      command: 'create',
      path: 'src/foo.ts',
      content: 'hello\nworld',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-denied-create',
          role: 'tool_use',
          name: 'fs_write',
          content,
          isFinished: true,
          status: 'rejected',
        } as any,
        'kiro_default'
      )
    );
    // Header carries DENIED.
    expect(out).toContain('DENIED');
    // Diff body present — line-numbered additions for the new file.
    expect(out).toMatch(/1 \+\s+hello/);
    expect(out).toMatch(/2 \+\s+world/);
    // Must NOT fall back to the raw args tree printer.
    expect(out).not.toMatch(/^\s*command:\s*create/m);
    expect(out).not.toMatch(/^\s*content:\s*hello/m);
  });

  test('denied fs_write strReplace renders as a diff with -/+ gutter', () => {
    const content = JSON.stringify({
      command: 'strReplace',
      path: 'src/bar.ts',
      oldStr: 'old line',
      newStr: 'new line',
    });
    const out = stripAnsi(
      renderMessageToText(
        {
          id: 't-denied-edit',
          role: 'tool_use',
          name: 'fs_write',
          content,
          isFinished: true,
          status: 'rejected',
        } as any,
        'kiro_default'
      )
    );
    expect(out).toContain('DENIED');
    expect(out).toMatch(/-\s*old line/);
    expect(out).toMatch(/\+\s*new line/);
    expect(out).not.toMatch(/^\s*oldStr:\s*old line/m);
    expect(out).not.toMatch(/^\s*newStr:\s*new line/m);
  });
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
