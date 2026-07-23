import React from 'react';
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest';
import { ToolUseMessage, type ToolUseMessageProps } from '../ToolUseMessage.js';
import { VerbosityToolContext } from '../VerbosityToolContext.js';
import { SubagentDetail } from '../../chat/tools/SubagentDetail.js';
import { SessionTool } from '../../chat/tools/SessionTool.js';
import { ToolUseStatus } from '../../../stores/app-store.js';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import { renderWithProviders as wrap } from '../../chat/tools/__tests__/twinki-render.js';
import {
  getTuiVerboseDisplay,
  resetVerboseCache,
  setVerboseConfig,
  TUI_DEFAULT_DISPLAY,
} from '../../../lite/verbose.js';
import { useTempKiroHome } from '../../../lite/__tests__/temp-kiro-home.js';
import { chalk } from '../../../utils/color.js';

chalk.level = 3;
useTempKiroHome();
beforeEach(resetVerboseCache);
afterEach(() => vi.restoreAllMocks());

type Display = NonNullable<Parameters<typeof setVerboseConfig>[0]['display']>;
type Result = NonNullable<ToolUseMessageProps['result']>;
type Message = Omit<ToolUseMessageProps, 'id' | 'content'> & {
  id?: string;
  args?: unknown;
};
type Checks = {
  has?: readonly string[];
  lower?: readonly string[];
  lacks?: readonly string[];
  matches?: readonly RegExp[];
  rejects?: readonly RegExp[];
  order?: readonly string[];
};
type RenderCase = [
  name: string,
  message: Message,
  checks: Checks,
  display?: Display,
  filters?: string[],
  outlines?: number | 'some',
];
const row = (
  name: string,
  message: Message,
  checks: Checks,
  display?: Display,
  filters?: string[],
  outlines?: number | 'some'
): RenderCase => [name, message, checks, display, filters, outlines];

const ok = (output: unknown) => ({ status: 'success', output }) as Result;
const err = (error: string) => ({ status: 'error', error }) as Result;
const texts = (...values: string[]) => ({
  items: values.map((Text) => ({ Text })),
});
const json = (Json: unknown) => ({ items: [{ Json }] });
const config = (display: Display = {}, filters: string[] = ['all']) =>
  setVerboseConfig({ filters, display }, 'tui');
const message = (
  name: string,
  args: unknown = {},
  output?: unknown,
  props: Partial<Message> = {}
): Message => ({
  name,
  args,
  ...(output === undefined ? {} : { result: ok(output) }),
  ...props,
});

async function render(
  { id = 'fixture', args = {}, ...props }: Message,
  expanded = false,
  columns = 120
): Promise<string> {
  return wrap(
    <ToolUseMessage
      id={id}
      content={JSON.stringify(args)}
      isFinished
      isStatic={false}
      {...props}
    />,
    {
      columns,
      configureStore: (store) =>
        store.setState({ toolOutputsExpanded: expanded }),
    }
  );
}

function check(
  text: string,
  {
    has = [],
    lower = [],
    lacks = [],
    matches = [],
    rejects = [],
    order = [],
  }: Checks
): void {
  for (const value of has) expect(text).toContain(value);
  for (const value of lower) expect(text.toLowerCase()).toContain(value);
  for (const value of lacks) expect(text).not.toContain(value);
  for (const value of matches) expect(text).toMatch(value);
  for (const value of rejects) expect(text).not.toMatch(value);
  for (let i = 1; i < order.length; i++)
    expect(text.indexOf(order[i - 1]!)).toBeLessThan(text.indexOf(order[i]!));
}

function cohort(enabled: boolean): void {
  if (enabled) process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
  else delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
  resetVerboseCache();
}

async function offCohort<T>(run: () => Promise<T>): Promise<T> {
  cohort(false);
  try {
    return await run();
  } finally {
    cohort(true);
  }
}

const PURPOSE = 'WHY_MARKER_REASONING';
const OUT = (i: number) => `OUTLINE_${i}`;
const STDOUT = Array.from({ length: 10 }, (_, i) => OUT(i + 1)).join('\n');
const outlineCount = (out: string) =>
  Array.from({ length: 10 }, (_, i) => OUT(i + 1)).filter((line) =>
    out.includes(line)
  ).length;
const SHELL = message(
  'execute_bash',
  { command: 'printf_marker_cmd', __tool_use_purpose: PURPOSE },
  { stdout: STDOUT, exit_status: 'exit status: 0' },
  { purpose: PURPOSE, startTime: 1000, finishTime: 2500 }
);
const ARGS = ['ARGVALUE_MARKER', 'SECOND_ARG_MARKER'];
const GENERIC = message(
  'mcp__demo__lookup',
  { region: ARGS[0], extra: ARGS[1], __tool_use_purpose: PURPOSE },
  { text: 'ok' },
  { purpose: PURPOSE }
);
const DIFF = 'DIFFBODY_MARKER_LINE';
const WRITE = message(
  'fs_write',
  { command: 'create', path: '/tmp/marker.ts' },
  undefined,
  {
    kind: 'edit',
    diff: {
      path: '/tmp/marker.ts',
      oldText: '',
      newText: `const ${DIFF} = 1;\n`,
    },
  }
);
const STATIC_WRITE = message(
  'fs_write',
  { command: 'create', path: '/tmp/static-marker.ts' },
  undefined,
  {
    kind: 'edit',
    isStatic: true,
    diff: {
      path: '/tmp/static-marker.ts',
      oldText: '',
      newText: Array.from(
        { length: 8 },
        (_, i) => `STATIC_WRITE_LINE_${i + 1}`
      ).join('\n'),
    },
  }
);
const READ = 'READ_BODY_MARKER_LINE';
const READ_ARGS = {
  operations: [{ mode: 'Line', path: 'src/x.ts', offset: 0 }],
};
const read = (output: unknown, props: Partial<Message> = {}) =>
  message('fs_read', READ_ARGS, output, {
    kind: 'read',
    isStatic: true,
    ...props,
  });
const KNOW = 'KNOWLEDGE_BODY_MARKER';
const INTROSPECT = 'INTROSPECT_DOC_MARKER';
const INTROSPECT_MESSAGE = message(
  'introspect',
  { query: 'how fs_read works' },
  {
    documentation: [
      INTROSPECT,
      ...Array.from({ length: 7 }, (_, i) => `doc line ${i + 2}`),
    ].join('\n'),
    query_context: 'how fs_read works',
  },
  { kind: 'read' }
);
const TWELVE = Array.from({ length: 12 }, (_, i) => `readline ${i + 1}`).join(
  '\n'
);
const MEGA = `{"overview":"${'x'.repeat(600)}"}`;

const INLINE: Display = { toolArgsMode: 'inline', persistOutput: true };
const CODE_INTEL = (body: string) =>
  message(
    'Code Intelligence',
    { operation: 'generate_codebase_overview' },
    texts(body),
    { kind: 'read' }
  );

// Keep the behavior matrix compact enough to scan one branch per row.
// prettier-ignore
const CASES: RenderCase[] = [
  row('zero config baseline', SHELL, { lacks: [PURPOSE, '1.5s'] }, undefined, undefined, 'some'),
  row('full shell config leaves output unbounded', SHELL, { has: [PURPOSE, 'printf_marker_cmd', '1.5s', OUT(1), OUT(10)], lacks: ['ctrl+o'] }, { toolArgsMode: 'block', showToolReasoning: true, showElapsed: true, outputMaxLines: null, outputMaxChars: null }, undefined, 10),
  row('full generic config leaves output unbounded', message('use_aws', {}, { stdout: STDOUT, exit_status: '0' }), { has: [OUT(1), OUT(10)], lacks: ['ctrl+o'] }, { outputMaxLines: null, outputMaxChars: null }),
  row('header-only shell config keeps its header', SHELL, { lower: ['shell'], lacks: [PURPOSE, '1.5s'] }, { toolArgsMode: 'off', showToolReasoning: false, showElapsed: false, outputMaxLines: 5 }, [], 0),
  ...([3, 8] as const).map((lines) => row(`shell outputMaxLines ${lines} is a soft cap`, SHELL, { lower: ['ctrl+o'], lacks: ['(capped)'] }, { outputMaxLines: lines, showToolReasoning: false }, undefined, lines)),
  ...(['inline', 'block', 'off'] as const).map((mode) => row(`generic args ${mode}`, GENERIC, mode === 'off' ? { has: ['mcp__demo__lookup'], lacks: ARGS } : { has: ARGS }, { toolArgsMode: mode, showToolReasoning: false, showElapsed: false }, [])),
  row('generic arg char cap applies to the complete CSV row', message('mcp__demo__lookup', { first: 'FIRST_ARG_MARKER', second: 'SECOND_ARG_MARKER' }), { has: ['first=FIRST_ARG_MARKER', '…'], lacks: ['SECOND_ARG_MARKER'] }, { toolArgsMode: 'block', argsMaxChars: 24, argsMaxLines: null }, []),
  row('multiline shell target respects argsMaxLines', message('execute_bash', { command: Array.from({ length: 7 }, (_, i) => `COMMAND_LINE_${i + 1}`).join('\n') }, { stdout: 'ok', exit_status: 'exit status: 0' }, { status: ToolUseStatus.Approved }), { has: ['COMMAND_LINE_1', 'COMMAND_LINE_2', '... (+5 more lines)'], lacks: ['COMMAND_LINE_3', 'COMMAND_LINE_7'] }, { argsMaxLines: 2, argsMaxChars: null, outputMaxLines: null }, []),
  row('generic no-arg output is capped without an empty arg block', message('mcp__demo__ping', {}, { text: 'NOARG_LINE_1\nNOARG_LINE_2\nNOARG_LINE_3' }), { has: ['mcp__demo__ping', 'output:', 'NOARG_LINE_1', 'NOARG_LINE_2', 'ctrl+o'], lacks: ['{}', 'NOARG_LINE_3'] }, { toolArgsMode: 'block', outputMaxLines: 2 }),
  row('generic JSON output renders only real embedded newlines', message('use_aws', {}, { stderr: 'ESCAPED_LINE_1\nESCAPED_LINE_2', literal: 'LITERAL\\nMARKER' }), { has: ['ESCAPED_LINE_1', 'ESCAPED_LINE_2', 'LITERAL\\\\nMARKER'], lacks: ['ESCAPED_LINE_1\\nESCAPED_LINE_2', 'LITERAL\nMARKER'] }, { outputMaxLines: 6 }),
  row('write diff enabled', WRITE, { has: [DIFF] }, { showWriteDiffs: true }, []),
  row('write diff disabled keeps header', WRITE, { lower: ['write'], lacks: [DIFF] }, { showWriteDiffs: false }, []),
  row('static persisted write diff has no stale cap hint', STATIC_WRITE, { has: ['STATIC_WRITE_LINE_1', 'STATIC_WRITE_LINE_8'], lacks: ['...+6 lines', 'ctrl+o'] }, { showWriteDiffs: true, persistOutput: true, outputMaxLines: 2 }, []),
  row('read body', read(texts(`const a = 1;\n${READ}\nconst c = 3;`)), { has: [READ] }, INLINE),
  row('read empty placeholder', read(texts('')), { has: ['(no output)'] }, INLINE),
  row('read ACP envelope', read({ content: [{ text: `x\n${READ}` }] }), { has: [READ] }, INLINE),
  row('batch read', read(texts('BATCH_A', 'BATCH_B'), { args: { paths: ['a.ts', 'b.ts'] } }), { has: ['BATCH_A', 'BATCH_B'] }, INLINE),
  row('directory read uses successful-output rendering', read(texts('-rw-r--r-- DIRECTORY_ENTRY'), { args: { operations: [{ mode: 'Directory', path: 'src/stores' }] }, isStatic: false }), { has: ['output:', '-rw-r--r-- DIRECTORY_ENTRY'], lacks: ['   1  -rw-r--r-- DIRECTORY_ENTRY'] }, INLINE),
  row('mixed directory and file reads preserve body styles', read(texts('-rw-r--r-- MIXED_DIRECTORY', 'const MIXED_SOURCE = true;'), { args: { operations: [{ mode: 'Directory', path: 'src' }, { mode: 'Line', path: 'src/x.ts' }] }, isStatic: false }), { has: ['-rw-r--r-- MIXED_DIRECTORY', '   1  const MIXED_SOURCE = true;'], lacks: ['   1  -rw-r--r-- MIXED_DIRECTORY'] }, INLINE),
  row('read output filter', read(texts(`line one\n${READ}`)), { lower: ['read'], lacks: [READ] }, INLINE, []),
  row('knowledge body', message('knowledge', { command: 'show' }, texts(`${KNOW}\nrow two`), { isStatic: true }), { has: [KNOW] }, INLINE),
  row('knowledge empty placeholder', message('knowledge', { command: 'show' }, texts(''), { isStatic: true }), { has: ['(no output)'] }, INLINE),
  row('knowledge whitespace placeholder', message('knowledge', { command: 'show' }, texts('\n\n'), { isStatic: true }), { has: ['(no output)'] }, INLINE),
  row('task output section', message('todo_list', { command: 'create' }, texts('TASK_BODY_MARKER'), { isStatic: true }), { has: ['output:', 'TASK_BODY_MARKER'] }, INLINE),
  row('introspect enabled', INTROSPECT_MESSAGE, { lower: ['introspect'], has: [INTROSPECT] }, INLINE, ['introspect']),
  row('introspect filtered', INTROSPECT_MESSAGE, { lower: ['introspect'], lacks: [INTROSPECT] }, INLINE, []),
  row('code summary full config is unbounded', message('Code Intelligence', { operation: 'lookup_symbols' }, texts(TWELVE), { kind: 'read' }), { has: ['readline 1', 'readline 12'] }, { ...INLINE, outputMaxLines: null }),
  row('read line cap', read(texts(TWELVE), { isStatic: false }), { has: ['output:', 'readline 12', 'ctrl+o'], lacks: ['readline 1\n'], matches: [/\+7 lines/] }, { ...INLINE, outputMaxLines: 5 }),
  row('code-intel line cap', CODE_INTEL(TWELVE), { has: ['output:', 'readline 12', 'ctrl+o'], lacks: ['  1  readline'], matches: [/\+7 lines/] }, { ...INLINE, outputMaxLines: 5 }),
  row('code-intel mega-line cap', CODE_INTEL(MEGA), { has: ['ctrl+o'], lacks: ['x'.repeat(200)], matches: [/\+\d+ lines above/] }, { ...INLINE, outputMaxLines: 4 }),
  row('in-cohort read error section', message('fs_read', { operations: [{ path: 'src/x.ts' }] }, undefined, { kind: 'read', status: ToolUseStatus.Approved, result: err('IN_READ_ERR_MARKER') }), { has: ['output:', 'IN_READ_ERR_MARKER'] }, INLINE),
  row('in-cohort blank task placeholder', message('task', { command: 'list' }, { text: '\r\n  \r\n' }), { has: ['(no output)'] }, INLINE),
];

test.each(CASES)(
  '%s',
  async (_name, msg, checks, display, filters, outlines) => {
    if (display) config(display, filters);
    const out = await render(msg);
    check(out, checks);
    if (outlines === 'some') expect(outlineCount(out)).toBeGreaterThan(0);
    else if (typeof outlines === 'number')
      expect(outlineCount(out)).toBe(outlines);
  }
);

test('dispatcher caps output, Ctrl+O expands it, and errors bypass caps', async () => {
  // prettier-ignore
  const capped = message('mcp__demo__ping', {}, { text: 'FIRST_ABCDEFGHIJ\nSECOND_KLMNOPQRST' });
  // prettier-ignore
  const error = message('mcp__demo__ping', {}, undefined, { result: err('GENERIC_ERR_MARKER_UNCLIPPED') });
  try {
    config({
      outputMaxLines: 1,
      outputMaxChars: 8,
      persistOutput: true,
    });
    check(await render(capped), {
      has: ['FIRST_A…', 'ctrl+o'],
      lacks: ['SECOND_…'],
    });
    check(await render(capped, true), {
      has: ['FIRST_ABCDEFGHIJ', 'SECOND_KLMNOPQRST'],
      lacks: ['ctrl+o', 'FIRST_A…', 'SECOND_…'],
    });
    check(await render({ ...capped, isStatic: true }), {
      has: ['FIRST_A…', '...+1 lines'],
      lacks: ['SECOND_…', 'ctrl+o'],
    });
    config({}, []);
    check(await render(error), { has: ['GENERIC_ERR_MARKER_UNCLIPPED'] });
  } finally {
    config({ outputMaxLines: null, outputMaxChars: null });
  }
});

test('character-only output truncation registers Ctrl+O', async () => {
  const msg = message(
    'mcp__demo__ping',
    {},
    {
      text: ['CHARACTER_ONLY_MARKER', '界界界界界', '👍🏽👍🏽👍🏽👍🏽👍🏽'].join('\n'),
    }
  );
  let hasExpandableOutput = () => false;
  try {
    config({
      outputMaxLines: null,
      outputMaxChars: 8,
      persistOutput: true,
    });
    const { id = 'char-only', args = {}, ...props } = msg;
    const collapsed = await wrap(
      <ToolUseMessage
        id={id}
        content={JSON.stringify(args)}
        isFinished
        isStatic={false}
        {...props}
      />,
      {
        configureStore: (store) => {
          hasExpandableOutput = () => store.getState().hasExpandableToolOutputs;
        },
      }
    );
    check(collapsed, {
      has: ['CHARACT…', '界界界…', '👍🏽👍🏽👍🏽…', 'ctrl+o'],
      lacks: ['CHARACTER_ONLY_MARKER', '界界界界界'],
    });
    expect(hasExpandableOutput()).toBe(true);
    check(await render(msg, true), {
      has: ['CHARACTER_ONLY_MARKER', '界界界界界', '👍🏽👍🏽👍🏽👍🏽👍🏽'],
      lacks: ['CHARACT…', '界界界…', 'ctrl+o'],
    });
  } finally {
    config({ outputMaxLines: null, outputMaxChars: null });
  }
});

test('Ctrl+O expands line- and character-capped tool arguments', async () => {
  const shell = message(
    'execute_bash',
    {
      command: Array.from(
        { length: 7 },
        (_, i) => `EXPANDED_COMMAND_LINE_${i + 1}`
      ).join('\n'),
    },
    { stdout: 'ok', exit_status: 'exit status: 0' },
    { status: ToolUseStatus.Approved }
  );
  const generic = message('mcp__demo__lookup', {
    first: 'FIRST_ARG_MARKER',
    second: 'SECOND_ARG_MARKER',
  });
  try {
    config({ argsMaxLines: 2, argsMaxChars: 12, outputMaxLines: null }, []);
    check(await render(shell), {
      has: ['EXPANDED_CO…', '... (+5 more lines)', 'ctrl+o'],
      lacks: ['EXPANDED_COMMAND_LINE_3', 'EXPANDED_COMMAND_LINE_7'],
    });
    check(await render(shell, true), {
      has: ['EXPANDED_COMMAND_LINE_1', 'EXPANDED_COMMAND_LINE_7'],
      lacks: ['more lines', 'ctrl+o'],
    });

    config({ argsMaxLines: null, argsMaxChars: 24 }, []);
    check(await render(generic), {
      has: ['first=FIRST_ARG_MARKER', 'ctrl+o'],
      lacks: ['SECOND_ARG_MARKER'],
    });
    check(await render(generic, true), {
      has: ['first=FIRST_ARG_MARKER', 'second=SECOND_ARG_MARKER'],
      lacks: ['ctrl+o'],
    });
  } finally {
    config({
      toolArgsMode: 'block',
      argsMaxLines: null,
      argsMaxChars: null,
    });
  }
});

test('hidden arguments do not register Ctrl+O', async () => {
  const messages = [
    message('mcp__demo__lookup', { first: 'HIDDEN_LONG_ARGUMENT' }),
    message(
      'Sub-agent: worker',
      { prompt: 'HIDDEN_COLLAPSED_ARGUMENT' },
      undefined,
      { kind: 'other' }
    ),
  ];
  try {
    config({ toolArgsMode: 'off', argsMaxChars: 4 }, []);
    for (const msg of messages) {
      let hasExpandableOutput = () => true;
      const { id = 'hidden-args', args = {}, ...props } = msg;
      const out = await wrap(
        <ToolUseMessage
          id={id}
          content={JSON.stringify(args)}
          isFinished
          isStatic={false}
          {...props}
        />,
        {
          configureStore: (store) => {
            hasExpandableOutput = () =>
              store.getState().hasExpandableToolOutputs;
          },
        }
      );
      check(out, {
        lacks: ['HIDDEN_LONG_ARGUMENT', 'HIDDEN_COLLAPSED_ARGUMENT', 'ctrl+o'],
      });
      expect(hasExpandableOutput()).toBe(false);
    }
  } finally {
    config({ toolArgsMode: 'block', argsMaxChars: null });
  }
});

test('filtered outputs do not register Ctrl+O', async () => {
  const hiddenOutput = 'HIDDEN_OUTPUT_MARKER_1\nHIDDEN_OUTPUT_MARKER_2';
  const messages = [
    message('mcp__demo__lookup', {}, { text: hiddenOutput }),
    message(
      'execute_bash',
      { command: 'emit-hidden-output' },
      { stdout: hiddenOutput, exit_status: 'exit status: 0' },
      { status: ToolUseStatus.Approved }
    ),
    read(texts(hiddenOutput), {
      args: {
        operations: [
          { mode: 'Line', path: 'src/one.ts' },
          { mode: 'Line', path: 'src/two.ts' },
        ],
      },
    }),
    message(
      'ls',
      { path: '/tmp' },
      texts(
        '-rw-r--r-- 1 0 0 1 Jan 1 00:00 /tmp/HIDDEN_OUTPUT_MARKER_1',
        '-rw-r--r-- 1 0 0 1 Jan 1 00:00 /tmp/HIDDEN_OUTPUT_MARKER_2'
      )
    ),
  ];
  try {
    config(
      {
        toolArgsMode: 'block',
        argsMaxLines: null,
        argsMaxChars: null,
        outputMaxLines: 1,
        outputMaxChars: 8,
      },
      []
    );
    for (const msg of messages) {
      let hasExpandableOutput = () => true;
      const { id = 'hidden-output', args = {}, ...props } = msg;
      const out = await wrap(
        <ToolUseMessage
          id={id}
          content={JSON.stringify(args)}
          isFinished
          isStatic={false}
          {...props}
        />,
        {
          configureStore: (store) => {
            hasExpandableOutput = () =>
              store.getState().hasExpandableToolOutputs;
          },
        }
      );
      check(out, { lacks: ['HIDDEN_OUTPUT_MARKER', 'ctrl+o'] });
      expect(hasExpandableOutput()).toBe(false);
    }
  } finally {
    config({ outputMaxLines: null, outputMaxChars: null });
  }
});

test('character-only caps expand specialized output bodies', async () => {
  const marker = 'SPECIALIZED_CHARACTER_MARKER';
  const cases = [
    message(
      'execute_bash',
      { command: 'printf-specialized-output' },
      { stdout: marker, exit_status: 'exit status: 0' },
      { status: ToolUseStatus.Approved }
    ),
    read(texts(marker), {
      args: { operations: [{ mode: 'Directory', path: 'src/stores' }] },
      isStatic: false,
    }),
    read(texts(marker), {
      args: { operations: [{ mode: 'Line', path: 'src/index.ts' }] },
      isStatic: false,
    }),
    message(
      'ls',
      { path: '/tmp' },
      texts(`-rw-r--r-- 1 0 0 1 Jan 1 00:00 /tmp/${marker}`)
    ),
  ];
  try {
    config({
      outputMaxLines: null,
      outputMaxChars: 8,
      persistOutput: true,
    });
    for (const msg of cases) {
      check(await render(msg), {
        has: ['SPECIAL…', 'ctrl+o'],
        lacks: [marker],
      });
      check(await render(msg, true), {
        has: [marker],
        lacks: ['SPECIAL…', 'ctrl+o'],
      });
    }
  } finally {
    config({ outputMaxLines: null, outputMaxChars: null });
  }
});

test('finite shell caps do not recap expanded output at 1,000 lines', async () => {
  const lines = Array.from(
    { length: 1001 },
    (_, i) => `EXPANDED_SHELL_LINE_${i + 1}`
  ).join('\n');
  try {
    config({ outputMaxLines: 5, outputMaxChars: null });
    const out = await render(
      message(
        'execute_bash',
        { command: 'emit-many-lines' },
        { stdout: lines, exit_status: 'exit status: 0' },
        { status: ToolUseStatus.Approved }
      ),
      true
    );
    check(out, {
      has: ['EXPANDED_SHELL_LINE_1', 'EXPANDED_SHELL_LINE_1001'],
      lacks: ['[truncated', 'ctrl+o'],
    });
  } finally {
    config({ outputMaxLines: null, outputMaxChars: null });
  }
});

test('read collapses to header-only when static + persistOutput off', async () => {
  config({ toolArgsMode: 'inline', persistOutput: false });
  try {
    check(await render(read(texts(READ))), { lacks: [READ] });
  } finally {
    setVerboseConfig({ display: { persistOutput: true } }, 'tui');
  }
});

describe('off the rollout cohort', () => {
  beforeEach(() => cohort(false));
  afterEach(() => cohort(true));

  // prettier-ignore
  const cases: RenderCase[] = [
    row('introspect header only', INTROSPECT_MESSAGE, { lower: ['introspect'], lacks: [INTROSPECT] }, INLINE),
    row('snake_case write args without diff', message('fs_write', { command: 'str_replace', path: '/tmp/x.ts', old_str: 'OLD_SNAKE_MARKER', new_str: 'NEW_SNAKE_MARKER' }, undefined, { kind: 'edit', isStatic: true }), { has: ['old_str=OLD_SNAKE_MARKER', 'new_str=NEW_SNAKE_MARKER'], rejects: [/added \d+ line/, /removed \d+ line/] }, { toolArgsMode: 'block' }),
    row('insert_line write arg', message('fs_write', { command: 'insert', path: '/tmp/x.ts', insert_line: 10 }, undefined, { kind: 'edit', isStatic: true }), { has: ['insert_line=10'] }, { toolArgsMode: 'block' }),
    row('read header only', read(texts('OFF_READ_BODY')), { lower: ['read'], lacks: ['OFF_READ_BODY', '(no output)'] }, INLINE),
    row('read error is bare', message('fs_read', { operations: [{ path: 'src/x.ts' }] }, undefined, { kind: 'read', status: ToolUseStatus.Approved, result: err('OFF_READ_ERR_MARKER') }), { has: ['OFF_READ_ERR_MARKER'], lacks: ['output:'] }, INLINE),
    row('shell output is bare', message('execute_bash', { command: 'ls' }, { stdout: 'SHELL_OUT_MARKER', exit_status: 'exit status: 0' }, { status: ToolUseStatus.Approved }), { has: ['SHELL_OUT_MARKER'], lacks: ['output:'] }, INLINE),
    row('generic output is bare', message('mcp__demo__lookup', { q: 1 }, { text: 'TOOL_OUT_MARKER' }, { status: ToolUseStatus.Approved }), { has: ['TOOL_OUT_MARKER'], lacks: ['output:'] }, INLINE),
    row('structured generic output keeps JSON escapes', message('use_aws', {}, { stdout: 'OFF_JSON_ONE\nOFF_JSON_TWO', exit_status: '0' }), { has: ['OFF_JSON_ONE\\nOFF_JSON_TWO'], lacks: ['OFF_JSON_ONE\nOFF_JSON_TWO'] }, INLINE),
    row('glob keeps its three-file preview', message('glob', { pattern: '**/*' }, json({ filePaths: ['OFF_GLOB_1', 'OFF_GLOB_2', 'OFF_GLOB_3', 'OFF_GLOB_4', 'OFF_GLOB_5'], totalFiles: 5, truncated: false })), { has: ['OFF_GLOB_1', 'OFF_GLOB_3', 'ctrl+o'], lacks: ['OFF_GLOB_4', 'OFF_GLOB_5'] }, { ...INLINE, outputMaxLines: null }),
  ];

  test.each(cases)(
    '%s (mainline parity)',
    async (_name, msg, checks, display) => {
      config(display);
      check(await render(msg), checks);
    }
  );

  test('hidden introspect output does not register Ctrl+O', async () => {
    const {
      id = 'hidden-introspect',
      args = {},
      ...props
    } = INTROSPECT_MESSAGE;
    let hasExpandableOutput = () => true;
    await wrap(
      <ToolUseMessage
        id={id}
        content={JSON.stringify(args)}
        isFinished
        isStatic={false}
        {...props}
      />,
      {
        configureStore: (store) => {
          hasExpandableOutput = () => store.getState().hasExpandableToolOutputs;
        },
      }
    );
    expect(hasExpandableOutput()).toBe(false);
  });

  test('narrow terminals keep legacy natural argument wrapping', async () => {
    config({ argsMaxLines: 1, argsMaxChars: 4 });
    const out = await render(
      message('mcp__demo__lookup', {
        first: 'OFF_FIRST_ARGUMENT',
        second: 'OFF_SECOND_ARGUMENT',
      }),
      false,
      28
    );
    expect(out.replace(/\s/g, '')).toContain(
      'first=OFF_FIRST_ARGUMENT,second=OFF_SECOND_ARGUMENT'
    );
    check(out, { lacks: ['more lines', 'ctrl+o'] });
  });

  test('static lists keep the legacy suffix without a Ctrl+O hint', async () => {
    const out = await render(
      message(
        'glob',
        { pattern: '**/*' },
        json({
          filePaths: [
            'OFF_STATIC_1',
            'OFF_STATIC_2',
            'OFF_STATIC_3',
            'OFF_STATIC_4',
            'OFF_STATIC_5',
          ],
          totalFiles: 5,
          truncated: false,
        }),
        { isStatic: true }
      )
    );
    check(out, {
      has: ['OFF_STATIC_1', 'OFF_STATIC_3', '+2 more'],
      lacks: ['OFF_STATIC_4', 'ctrl+o'],
    });
  });

  test('expanded generic output ignores saved character caps', async () => {
    try {
      config({ outputMaxChars: 4 });
      check(
        await render(
          message('mcp__demo__lookup', {}, { text: 'OFF_EXPANDED_FULL' }),
          true
        ),
        { has: ['OFF_EXPANDED_FULL'], lacks: ['OFF_…'] }
      );
    } finally {
      config({ outputMaxChars: null });
    }
  });
});

type Search = {
  title: string;
  name: 'glob' | 'grep';
  reason: string;
  body: string;
  args: Record<string, unknown>;
  output: unknown;
};
const SEARCHES: Search[] = [
  {
    title: 'Glob',
    name: 'glob',
    reason: 'GLOB_WHY_MARKER',
    body: 'GLOBFILE_MARKER.ts',
    args: { pattern: '**/*verbose*' },
    output: json({
      filePaths: ['src/a/verbose.ts', 'src/b/GLOBFILE_MARKER.ts'],
      totalFiles: 2,
      truncated: false,
    }),
  },
  {
    title: 'Grep',
    name: 'grep',
    reason: 'GREP_WHY_MARKER',
    body: 'GREPMATCH_MARKER',
    args: { pattern: 'verbose' },
    output: json({
      numMatches: 3,
      numFiles: 1,
      truncated: false,
      results: [
        {
          file: 'src/consts.ts',
          count: 3,
          matches: ['1:GREPMATCH_MARKER verbose'],
        },
      ],
    }),
  },
];
const GREP = SEARCHES[1]!;
const renderSearch = (search: Search, isStatic: boolean) =>
  render(
    message(
      search.name,
      { ...search.args, __tool_use_purpose: search.reason },
      search.output,
      {
        purpose: search.reason,
        isStatic,
        startTime: 1000,
        finishTime: 1500,
      }
    )
  );

describe('search output persistence and headers', () => {
  test.each(SEARCHES)(
    '$title: reasoning order and static body persistence',
    async (search) => {
      config(
        {
          showToolReasoning: true,
          toolArgsMode: 'inline',
          showElapsed: true,
          outputMaxLines: 10,
          persistOutput: true,
        },
        ['grep', 'glob']
      );
      const live = await renderSearch(search, false);
      check(live, {
        has: [search.title, search.reason, search.body],
        order: [search.title, search.reason, search.body],
      });
      check(await renderSearch(search, true), { has: [search.body] });
    }
  );

  test('persistOutput off collapses grep on the static transition', async () => {
    config(
      {
        showToolReasoning: false,
        toolArgsMode: 'inline',
        persistOutput: false,
      },
      ['grep']
    );
    check(await renderSearch(GREP, false), { has: [GREP.body] });
    check(await renderSearch(GREP, true), {
      has: [GREP.title],
      lacks: [GREP.body, '3 matches in 1 file'],
    });
  });

  test.each(SEARCHES)(
    '$name results render under an output: header in-cohort',
    async (search) => {
      config({ toolArgsMode: 'inline', persistOutput: true }, ['grep', 'glob']);
      const out = await renderSearch(search, false);
      check(out, {
        has: ['output:', search.body],
        order: ['output:', search.body],
      });
    }
  );

  test('grep off-cohort has no output: header', async () => {
    const out = await offCohort(() => renderSearch(GREP, false));
    check(out, { has: [GREP.body], lacks: ['output:'] });
  });
});

const SUBAGENT_CONTENT = JSON.stringify({
  task: 'find X',
  stages: [{ name: 'stg', prompt_template: 'SUBAGENT_PROMPT_MARKER' }],
});
const SUBAGENT_SUMMARIES = [
  {
    stageName: 'stg',
    contextSummary: 'SUB_CTX_MARKER',
    taskResult: 'SUB_RAW_MARKER',
  },
];
async function renderSubagent(
  filters: string[],
  engine: 'v2' | 'kas' = 'v2',
  outputMaxChars: number | null = null,
  expanded = false
): Promise<string> {
  config(
    {
      toolArgsMode: 'block',
      outputMaxLines: null,
      outputMaxChars,
      subagent: {
        pipeline: true,
        prompts: true,
        roles: true,
        deps: true,
        responses: true,
      },
    },
    filters
  );
  return wrap(
    <VerbosityToolContext.Provider value={{ outputVisible: true }}>
      <SubagentDetail
        content={SUBAGENT_CONTENT}
        summaries={SUBAGENT_SUMMARIES as never}
        display={getTuiVerboseDisplay()}
        finished
        isKas={engine === 'kas'}
        showFullOutput={filters.includes('all') || filters.includes('subagent')}
      />
    </VerbosityToolContext.Provider>,
    {
      store: { agentEngine: engine } as never,
      configureStore: (store) =>
        store.setState({ toolOutputsExpanded: expanded }),
    }
  );
}

test.each([
  [
    'full output shows when the filter is on',
    ['all'],
    ['full output:', 'SUB_RAW_MARKER'],
    [],
  ],
  [
    'full output is hidden when the filter is off',
    [],
    [],
    ['full output:', 'SUB_RAW_MARKER'],
  ],
  [
    'prompt renders when output is filtered',
    [],
    ['SUBAGENT_PROMPT_MARKER'],
    [],
  ],
] as const)('subagent %s', async (_name, filters, has, lacks) => {
  check(await renderSubagent([...filters]), { has, lacks });
});

test('KAS suppresses the response summary: section', async () => {
  check(await renderSubagent(['all'], 'v2'), { has: ['response summary:'] });
  check(await renderSubagent(['all'], 'kas'), {
    has: ['full output:'],
    lacks: ['response summary:'],
  });
});

test('Ctrl+O expands character-capped subagent output', async () => {
  check(await renderSubagent(['all'], 'v2', 8), {
    has: ['SUB_RAW…', 'ctrl+o'],
    lacks: ['SUB_RAW_MARKER'],
  });
  check(await renderSubagent(['all'], 'v2', 8, true), {
    has: ['SUB_RAW_MARKER'],
    lacks: ['SUB_RAW…', 'ctrl+o'],
  });
});

test('TUI-default config hides subagent detail', async () => {
  const content = JSON.stringify({
    task: 'bounded task',
    stages: [{ name: 'worker', prompt_template: 'HIDDEN_PROMPT_MARKER' }],
  });
  config(TUI_DEFAULT_DISPLAY, ['all', '-subagent']);
  const out = await render(
    message('subagent', JSON.parse(content), texts('HIDDEN_OUTPUT_MARKER'), {
      id: 'default-subagent',
    })
  );
  check(out, {
    has: ['Orchestrated'],
    lacks: [
      'pipeline:',
      'HIDDEN_PROMPT_MARKER',
      'HIDDEN_OUTPUT_MARKER',
      'full output:',
    ],
  });
});

test.each([
  ['session_management', 0],
  ['subagent', 2],
] as const)('hidden %s subscriptions', async (name, expected) => {
  config({ subagent: { responses: false } }, []);
  let appSubscriptions = () => 0;
  const conversations = vi.spyOn(sessionConversationsStore, 'subscribe');
  await wrap(
    <VerbosityToolContext.Provider value={{ outputVisible: false }}>
      <SessionTool id="hidden-session" name={name} isFinished result={ok('')} />
    </VerbosityToolContext.Provider>,
    {
      configureStore: (store) => {
        const subscribe = vi.spyOn(store, 'subscribe');
        appSubscriptions = () => subscribe.mock.calls.length;
      },
    }
  );
  expect(appSubscriptions()).toBe(expected);
  expect(conversations).not.toHaveBeenCalled();
});

test('subagent detail is fully inert off-cohort (mainline had none)', async () => {
  expect((await offCohort(() => renderSubagent(['all']))).trim()).toBe('');
});
