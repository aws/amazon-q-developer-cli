import { describe, test, expect, beforeEach, afterAll } from 'vitest';
import { chalk } from '../../utils/color.js';
import {
  formatSubagentApprovalLines,
  renderSubagentFinalBlock,
} from '../render.js';
import {
  setVerboseConfig,
  resetVerboseCache,
  DEFAULT_DISPLAY,
  DENSITY_DISPLAY,
} from '../verbose.js';
import stripAnsi from 'strip-ansi';
import { useTempKiroHome } from './temp-kiro-home.js';
import { expectRender, section } from './expect-render.js';

useTempKiroHome();

const FULL_SUBAGENT_DISPLAY = DENSITY_DISPLAY.full;
const CAPPED_SUBAGENT_DISPLAY = {
  ...DENSITY_DISPLAY.full,
  subagent: { ...DENSITY_DISPLAY.full.subagent },
  outputMaxLines: 30,
};

describe('formatSubagentApprovalLines', () => {
  test('renders task line and per-stage tree with role and depends_on', () => {
    const content = JSON.stringify({
      task: 'Generate test output',
      stages: [
        {
          name: 'research-colors',
          role: 'gpu-minimal',
          prompt_template: 'List colors',
        },
        {
          name: 'research-icons',
          role: 'gpu-minimal',
          prompt_template: 'List icons',
        },
        {
          name: 'combine',
          role: 'gpu-minimal',
          prompt_template: 'Combine the findings',
          depends_on: ['research-colors', 'research-icons'],
        },
      ],
    });
    const lines = formatSubagentApprovalLines(content, 100);
    expect(lines).not.toBeNull();
    const text = (lines ?? []).map(stripAnsi).join('\n');
    expect(text).toContain('pipeline:');
    // Stage names render as [name] in the same blue as the footer activity
    // strip — matches the row in the strip the user is watching.
    expect(text).toContain('[research-colors]');
    expect(text).toContain('[combine]');
    expect(text).toContain('(gpu-minimal)');
    expect(text).toContain('← research-colors, research-icons');
    expect(text).toContain('List colors');
    // Branches: the first two stages should use ├─, the last └─.
    expect(text).toContain('├─');
    expect(text).toContain('└─');
  });

  test('returns null on invalid JSON', () => {
    expect(formatSubagentApprovalLines('not-json')).toBeNull();
    expect(formatSubagentApprovalLines('')).toBeNull();
  });

  test('long prompt wraps to terminal width without crashing to col 0', () => {
    const longPrompt =
      'this is a really really really long prompt template that should soft-wrap at the available terminal width when the renderer formats it for the approval prompt';
    const content = JSON.stringify({
      task: 't',
      stages: [{ name: 's1', prompt_template: longPrompt }],
    });
    const lines = formatSubagentApprovalLines(content, 50);
    expect(lines).not.toBeNull();
    const stripped = (lines ?? []).map(stripAnsi);
    // Every line stays within ~50 cols (allow a couple extra for ANSI/whitespace edge).
    for (const line of stripped) {
      expect(line.length).toBeLessThanOrEqual(54);
    }
    // Continuation rows must still be indented under the stem (4 spaces + "  ").
    const promptStart = stripped.findIndex((l) => /this is a really/.test(l));
    expect(promptStart).toBeGreaterThanOrEqual(0);
    // Subsequent prompt continuation rows should also be indented (start with 4+ spaces).
    for (let i = promptStart + 1; i < stripped.length; i++) {
      const l = stripped[i]!;
      if (!l.trim()) continue;
      expect(l.startsWith('    ')).toBe(true);
    }
  });

  test('prompt input stays literal and preserves paragraph breaks', () => {
    const content = JSON.stringify({
      task: 't',
      stages: [
        {
          name: 's1',
          prompt_template: 'Use **bold** markdown.\n\nSecond paragraph.',
        },
      ],
    });
    const lines = formatSubagentApprovalLines(content, 100);
    expect(lines).not.toBeNull();
    const rows = (lines ?? []).map(stripAnsi);
    expect(rows.join('\n')).toContain('**bold**');
    const firstIdx = rows.findIndex((l) => l.includes('bold'));
    const secondIdx = rows.findIndex((l) => l.includes('Second paragraph'));
    expect(secondIdx).toBeGreaterThan(firstIdx);
    expect(
      rows.slice(firstIdx + 1, secondIdx).some((l) => l.trim().length === 0)
    ).toBe(true);
  });

  test('pipeline label and prompt use primary text without prose soft-wraps', () => {
    const content = JSON.stringify({
      stages: [
        {
          name: 's1',
          prompt_template: 'Review the module\nand report findings.',
        },
      ],
    });
    const raw = formatSubagentApprovalLines(content, 100)!.join('\n');
    const prompt = 'Review the module and report findings.';
    expect(raw).toContain(chalk.white('  pipeline:'));
    expect(raw).toContain(chalk.white(prompt));
  });

  test('markdown-looking input remains literal across wraps and stages', () => {
    const longBold =
      '**' + 'this is a long bold span that should wrap across a row' + '**';
    const content = JSON.stringify({
      task: 't',
      stages: [
        { name: 's1', prompt_template: longBold },
        { name: 's2', prompt_template: 'plain.' },
      ],
    });
    const lines = formatSubagentApprovalLines(content, 40);
    expect(lines).not.toBeNull();
    const joined = (lines ?? []).join('\n');
    expect(stripAnsi(joined)).toContain('**this is a long bold');
    expect(stripAnsi(joined)).toContain('row**');
    const s2TagIdx = joined.indexOf('[s2]');
    expect(s2TagIdx).toBeGreaterThan(0);
    // eslint-disable-next-line no-control-regex
    const sgrRe = /\x1b\[(\d+)m/g;
    let boldDepth = 0;
    let m: RegExpExecArray | null;
    sgrRe.lastIndex = 0;
    while ((m = sgrRe.exec(joined)) !== null) {
      if (m.index >= s2TagIdx) break;
      const code = Number(m[1]);
      if (code === 1) boldDepth++;
      else if (code === 22 || code === 0) boldDepth = 0;
    }
    expect(boldDepth).toBe(0);
  });
});

describe('renderSubagentFinalBlock', () => {
  // Re-read the temp config so state from another test cannot flip output
  // gates mid-suite.
  beforeEach(() => {
    resetVerboseCache();
  });

  const baseContent = JSON.stringify({
    task: 'Generate test output',
    stages: [
      { name: 'a', role: 'gpu-minimal', prompt_template: 'List ten colors.' },
      { name: 'b', role: 'gpu-minimal', prompt_template: 'List ten icons.' },
      {
        name: 'combine',
        role: 'gpu-minimal',
        prompt_template: 'Combine the two lists.',
        depends_on: ['a', 'b'],
      },
    ],
  });
  const heavyResult = {
    status: 'success',
    output:
      'Pipeline completed: 3 stages finished.\n\n## combine\n\nFinal long body that we do not want.',
  };
  // Keep these baseline assertions independent of cross-file KIRO_HOME changes.
  const baselineOptions = {
    display: DEFAULT_DISPLAY,
    filtersOverride: [] as const,
  };

  test('Lite default shows detail while its output filter hides raw results', () => {
    const block = stripAnsi(
      renderSubagentFinalBlock(
        baseContent,
        heavyResult,
        'done',
        5200,
        [
          {
            stageName: 'a',
            contextSummary: 'stage summary',
            taskResult: 'raw output',
          },
        ],
        { filtersOverride: ['shell'] }
      )
    );
    expect(block).toContain('subagent');
    expect(block).toContain('5.2s');
    expect(block).toContain('pipeline:');
    expect(block).toContain('stage summary');
    expect(block).not.toContain('raw output');
  });

  test('full renders the pipeline tree while omitting the parent joiner result', () => {
    const block = renderSubagentFinalBlock(
      baseContent,
      heavyResult,
      'done',
      5200,
      undefined,
      { display: FULL_SUBAGENT_DISPLAY, filtersOverride: ['all'] }
    );
    const stripped = stripAnsi(block);
    expect(stripped).toContain('subagent');
    expect(stripped).toContain('5.2s');
    expect(stripped).toContain('pipeline:');
    expect(stripped).toContain('├─');
    expect(stripped).toContain('└─');
    expect(stripped).toContain('combine');
    expect(stripped).toContain('← a, b');
    expect(stripped).toContain('List ten colors.');
    // Long taskResult body is intentionally suppressed pending /verbose.
    expect(stripped).not.toContain('Pipeline completed: 3 stages finished.');
    expect(stripped).not.toContain('Final long body');
    // No responses section when no stageSummaries provided.
    expect(stripped).not.toContain('response summary:');
  });

  // Responses section: contextSummary wins, taskResult is the fallback, and
  // the configured output line cap controls the footnote.
  const LONG_BODY = Array.from({ length: 50 }, (_, i) => `line ${i + 1}`).join(
    '\n'
  );
  test.each<{
    name: string;
    stageSummaries: {
      stageName: string;
      contextSummary: string;
      taskResult: string;
    }[];
    contains: string[];
    absent?: string[];
  }>([
    {
      name: 'renders ▸ chips for each summarized stage',
      stageSummaries: [
        {
          stageName: 'a',
          contextSummary: 'Colors: red, green, blue.',
          taskResult: '',
        },
        { stageName: 'b', contextSummary: 'Icons: ✓, ✗, ●.', taskResult: '' },
        {
          stageName: 'combine',
          contextSummary: 'Use red ✗ for errors.',
          taskResult: '',
        },
      ],
      contains: [
        'response summary:',
        '▸ a',
        'Colors: red, green, blue.',
        '▸ b',
        'Icons: ✓, ✗, ●.',
        '▸ combine',
        'Use red ✗ for errors.',
      ],
      // No markdown header noise; parent-tool joiner output stays hidden.
      absent: ['## a', '## combine', 'Final long body'],
    },
    {
      name: 'falls back to taskResult when contextSummary is empty',
      stageSummaries: [
        {
          stageName: 'a',
          contextSummary: '',
          taskResult: '42 typescript files.',
        },
        { stageName: 'b', contextSummary: '   ', taskResult: 'main' },
        {
          stageName: 'combine',
          contextSummary: 'Already digested.',
          taskResult: 'long body ignored',
        },
      ],
      // Empty contextSummary → taskResult renders; non-empty wins over taskResult.
      contains: [
        'response summary:',
        '▸ a',
        '42 typescript files.',
        '▸ b',
        'main',
        '▸ combine',
        'Already digested.',
      ],
      absent: ['long body ignored'],
    },
    {
      name: 'caps taskResult fallback at 30 lines with overflow footnote',
      stageSummaries: [
        { stageName: 'a', contextSummary: '', taskResult: LONG_BODY },
      ],
      contains: ['▸ a', 'line 1', 'line 30', '(+20 more lines)'],
      absent: ['line 31', 'line 50'],
    },
    {
      name: 'omits responses section entirely when every stage is empty',
      stageSummaries: [
        { stageName: 'a', contextSummary: '', taskResult: '' },
        { stageName: 'b', contextSummary: '', taskResult: '' },
      ],
      contains: [],
      absent: ['response summary:', '┌─ error:'],
    },
    {
      // Fully-empty stages get no chip, but a later non-empty stage still
      // renders — stage chips only appear in the responses section, so the
      // whole-output absent check is equivalent to scoping it there.
      name: 'skips stages with neither contextSummary nor taskResult',
      stageSummaries: [
        { stageName: 'a', contextSummary: '', taskResult: '' },
        { stageName: 'b', contextSummary: '   ', taskResult: '   ' },
        {
          stageName: 'combine',
          contextSummary: 'The only summary that matters.',
          taskResult: '',
        },
      ],
      contains: [
        'response summary:',
        '▸ combine',
        'The only summary that matters.',
      ],
      absent: ['▸ a', '▸ b'],
    },
  ])('$name', ({ stageSummaries, contains, absent }) => {
    expectRender(
      renderSubagentFinalBlock(
        baseContent,
        heavyResult,
        'done',
        1000,
        stageSummaries,
        { display: CAPPED_SUBAGENT_DISPLAY, filtersOverride: ['shell'] }
      ),
      { contains, absent }
    );
  });

  test('renders summaries in pipeline declaration order, not arrival order', () => {
    const block = stripAnsi(
      renderSubagentFinalBlock(
        baseContent,
        heavyResult,
        'done',
        1000,
        [
          {
            stageName: 'combine',
            contextSummary: 'third',
            taskResult: '',
          },
          { stageName: 'b', contextSummary: 'second', taskResult: '' },
          { stageName: 'a', contextSummary: 'first', taskResult: '' },
        ],
        { display: FULL_SUBAGENT_DISPLAY, filtersOverride: ['all'] }
      )
    );
    expect(block.indexOf('▸ a')).toBeLessThan(block.indexOf('▸ b'));
    expect(block.indexOf('▸ b')).toBeLessThan(block.indexOf('▸ combine'));
  });

  test('applies configured line and character caps to subagent digests', () => {
    const block = stripAnsi(
      renderSubagentFinalBlock(
        baseContent,
        heavyResult,
        'done',
        1000,
        [
          {
            stageName: 'a',
            contextSummary: 'abcdefghij\nsecond-long\nthird-line',
            taskResult: '',
          },
        ],
        {
          display: {
            ...FULL_SUBAGENT_DISPLAY,
            subagent: { ...FULL_SUBAGENT_DISPLAY.subagent },
            outputMaxLines: 2,
            outputMaxChars: 5,
          },
          filtersOverride: ['all'],
        }
      )
    );
    expect(block).toContain('abcd…');
    expect(block).toContain('seco…');
    expect(block).not.toContain('third-line');
    expect(block).toContain('(+1 more lines)');
  });

  test('counts wrapped digest rows toward outputMaxLines', () => {
    const block = stripAnsi(
      renderSubagentFinalBlock(
        baseContent,
        heavyResult,
        'done',
        1000,
        [
          {
            stageName: 'a',
            contextSummary: Array.from(
              { length: 30 },
              (_, index) => `wrapped-${index}`
            ).join(' '),
            taskResult: '',
          },
        ],
        {
          display: {
            ...FULL_SUBAGENT_DISPLAY,
            subagent: { ...FULL_SUBAGENT_DISPLAY.subagent },
            outputMaxLines: 1,
          },
          filtersOverride: ['all'],
        }
      )
    );
    expect(block).toContain('wrapped-0');
    expect(block).not.toContain('wrapped-29');
    expect(block).toMatch(/\(\+\d+ more lines\)/);
  });

  test.each([
    [
      'v2 summary',
      [
        {
          stageName: 'a',
          contextSummary: 'V2_STATIC_BODY',
          taskResult: '',
        },
      ],
      ['shell'],
      'V2_STATIC_BODY',
    ],
    [
      'KAS response',
      [
        {
          stageName: 'a',
          kind: 'response' as const,
          contextSummary: '',
          taskResult: 'KAS_STATIC_BODY',
        },
      ],
      ['all'],
      'KAS_STATIC_BODY',
    ],
  ] as const)(
    'persistOutput off hides static %s while preserving the pipeline',
    (_name, summaries, filtersOverride, body) => {
      const block = stripAnsi(
        renderSubagentFinalBlock(
          baseContent,
          heavyResult,
          'done',
          1000,
          summaries,
          {
            display: {
              ...FULL_SUBAGENT_DISPLAY,
              persistOutput: false,
              subagent: { ...FULL_SUBAGENT_DISPLAY.subagent },
            },
            filtersOverride,
            isStatic: true,
          }
        )
      );
      expect(block).toContain('pipeline:');
      expect(block).not.toContain(body);
    }
  );

  test('renders error state with FAILED tail and red-coloured body, even with stageSummaries', () => {
    const content = JSON.stringify({ task: 't', stages: [] });
    const result = {
      status: 'error',
      error: 'pipeline aborted: stage timeout',
    };
    const block = renderSubagentFinalBlock(
      content,
      result,
      'error',
      undefined,
      [
        {
          stageName: 'x',
          contextSummary: 'should not render on error',
          taskResult: '',
        },
      ],
      baselineOptions
    );
    const stripped = stripAnsi(block);
    expect(stripped).toContain('FAILED');
    expect(stripped).toContain('┌─ error:');
    expect(stripped).toContain('pipeline aborted: stage timeout');
    expect(stripped).not.toContain('response summary:');
    expect(stripped).not.toContain('should not render on error');
  });

  // Running-state header tail (empty stages so no tree branches confuse the
  // assertion). Precedence mirrors renderToolCall: a threaded runningSpinner
  // replaces ' ...', but an awaitingApproval flag wins with a yellow ' ...'
  // (`\x1b[33m`, the approval prompt's [t] hotkey color).
  test.each<{
    name: string;
    info?: { runningSpinner?: string; awaitingApproval?: boolean };
    containsMatch?: RegExp;
    contains?: string[];
    absent?: string[];
    absentMatch?: RegExp;
    rawContains?: string[];
  }>([
    {
      name: 'running state shows ... and no result/summary block',
      containsMatch: /subagent\s+\.\.\./,
      absent: ['┌─'],
    },
    {
      name: 'runningSpinner substitutes the spinner glyph (replaces ..., not concat)',
      info: { runningSpinner: '⠋' },
      contains: ['subagent', '⠋'],
      absentMatch: /subagent\s+\.\.\./,
    },
    {
      name: 'awaitingApproval overrides spinner with yellow ...',
      info: { runningSpinner: '⠋', awaitingApproval: true },
      contains: ['subagent', '...'],
      absent: ['⠋'],
      rawContains: ['\x1b[33m'],
    },
  ])(
    '$name',
    ({ info, containsMatch, contains, absent, absentMatch, rawContains }) => {
      const content = JSON.stringify({ task: 't', stages: [] });
      const block = renderSubagentFinalBlock(
        content,
        undefined,
        'running',
        undefined,
        undefined,
        { ...baselineOptions, ...info }
      );
      expectRender(block, {
        contains,
        absent,
        matches: containsMatch ? [containsMatch] : undefined,
        notMatches: absentMatch ? [absentMatch] : undefined,
        rawContains,
      });
    }
  );
});

describe('display.subagent section toggles', () => {
  beforeEach(() => {
    resetVerboseCache();
  });

  const subagentContent = JSON.stringify({
    task: 'Inspect codebase',
    stages: [
      {
        name: 'a',
        role: 'minimal',
        prompt_template: 'Look at file A.',
        depends_on: [],
      },
      {
        name: 'b',
        role: 'minimal',
        prompt_template: 'Look at file B.',
        depends_on: ['a'],
      },
    ],
  });
  const summaries = [
    { stageName: 'a', contextSummary: 'A digest body.', taskResult: '' },
    { stageName: 'b', contextSummary: 'B digest body.', taskResult: '' },
  ];
  const heavyResult = { status: 'success', output: 'Pipeline done.' };

  // Each case flips one or two subagent section toggles off and asserts what
  // disappears (absent) while the rest stays (contains). All sections start on.
  test.each([
    [
      'pipeline off hides the entire pipeline tree',
      { pipeline: false },
      ['response summary:', 'A digest body.'],
      ['pipeline:', '[a]'],
    ],
    [
      'prompts off hides per-stage prompt body but keeps the stage chips',
      { prompts: false },
      ['pipeline:', '[a]', '[b]'],
      ['Look at file A.', 'Look at file B.'],
    ],
    [
      'roles + deps off strip the role chip and ← deps annotations',
      { roles: false, deps: false },
      ['[a]'],
      ['(minimal)', '← a'],
    ],
    [
      'responses off suppresses the entire digest section',
      { responses: false },
      ['pipeline:'],
      ['response summary:', 'A digest body.', 'B digest body.'],
    ],
  ] as const)('%s', (_name, subagentToggles, contains, absent) => {
    const block = renderSubagentFinalBlock(
      subagentContent,
      heavyResult,
      'done',
      1000,
      summaries,
      {
        display: {
          ...FULL_SUBAGENT_DISPLAY,
          subagent: {
            ...FULL_SUBAGENT_DISPLAY.subagent,
            ...subagentToggles,
          },
          outputMaxLines: null,
          argsMaxChars: 80,
        },
      }
    );
    expectRender(block, { contains, absent });
  });
});

describe('renderSubagentFinalBlock verbose mode', () => {
  beforeEach(() => {
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
  });

  afterAll(() => {
    setVerboseConfig({ filters: [] });
    resetVerboseCache();
  });

  const subagentContent = JSON.stringify({
    task: 'Inspect codebase',
    stages: [
      { name: 'a', role: 'minimal', prompt_template: 'Look at file A.' },
      { name: 'b', role: 'minimal', prompt_template: 'Look at file B.' },
    ],
  });
  const heavyResult = {
    status: 'success',
    output: 'Pipeline done.',
  };

  // Raw "full output:" section is gated by the verbose filter list and only
  // renders the (uncapped) taskResult of non-empty stages; the "response
  // summary:" digest always renders. Order invariant: pipeline → raw output →
  // responses, so the same-named summary chip lands last.
  const HEAVY_RAW =
    'Long raw report from stage A.\n' +
    Array.from({ length: 60 }, (_, i) => `raw-line-${i}`).join('\n');
  type SubResult = { status: string; output?: string; error?: string };
  test.each<{
    name: string;
    filters: string[];
    result?: SubResult;
    status?: 'error' | 'cancelled' | 'running' | 'done';
    summaries: {
      stageName: string;
      contextSummary: string;
      taskResult: string;
    }[];
    contains?: string[];
    absent?: string[];
    rawContains?: string[];
    rawAbsent?: string[];
    responsesContains?: string[];
    requireRawBeforeResponses?: boolean;
    rawNoCap?: boolean;
  }>([
    {
      name: 'raw output section uses red ▸ chips with full (uncapped) taskResult',
      filters: ['subagent'],
      summaries: [
        { stageName: 'a', contextSummary: 'A is fine.', taskResult: HEAVY_RAW },
        {
          stageName: 'b',
          contextSummary: 'B is fine.',
          taskResult: 'Short raw from B.',
        },
      ],
      contains: ['full output:'],
      rawContains: ['▸ a', '▸ b', 'raw-line-0', 'raw-line-59'],
      responsesContains: ['A is fine.', 'B is fine.'],
      requireRawBeforeResponses: true,
      rawNoCap: true,
    },
    {
      name: 'filter not matching subagent: no raw output section',
      filters: ['shell'],
      summaries: [
        {
          stageName: 'a',
          contextSummary: 'A is fine.',
          taskResult: 'this should NOT appear',
        },
      ],
      absent: ['full output:', 'this should NOT appear'],
      contains: ['response summary:', 'A is fine.'],
    },
    {
      name: 'verbose off: compact summary only (no raw section)',
      filters: [],
      summaries: [
        {
          stageName: 'a',
          contextSummary: 'A digest',
          taskResult: 'long raw body that must stay hidden',
        },
      ],
      absent: ['full output:', 'long raw body that must stay hidden'],
      contains: ['response summary:', 'A digest'],
    },
    {
      name: 'stage with empty taskResult is skipped in raw section',
      filters: ['subagent'],
      summaries: [
        { stageName: 'a', contextSummary: 'digest A', taskResult: '' },
        { stageName: 'b', contextSummary: 'digest B', taskResult: 'has body' },
      ],
      rawAbsent: ['▸ a'],
      rawContains: ['▸ b', 'has body'],
      responsesContains: ['digest A', 'digest B'],
    },
    {
      // Raw "full output:" taskResult routes through the markdown pipeline:
      // markers strip in the rawSection slice (same pipeline pinned exhaustively
      // in render-markdown.test.ts).
      name: 'raw full output renders markdown (markers stripped) for taskResult',
      filters: ['subagent'],
      summaries: [
        {
          stageName: 'a',
          contextSummary: 'short digest',
          taskResult: '# Heading\n\n**Important** finding.',
        },
      ],
      rawContains: ['Heading', 'Important'],
      rawAbsent: ['# Heading', '**Important**'],
    },
    {
      name: 'error path: raw output suppressed; error block still renders',
      filters: ['subagent'],
      result: { status: 'error', error: 'stage timeout' },
      status: 'error',
      summaries: [
        { stageName: 'a', contextSummary: '', taskResult: 'should not appear' },
      ],
      absent: ['full output:', 'should not appear'],
      contains: ['FAILED', 'stage timeout'],
    },
  ])(
    'verbose: $name',
    ({
      filters,
      result,
      status,
      summaries,
      contains = [],
      absent = [],
      rawContains = [],
      rawAbsent = [],
      responsesContains = [],
      requireRawBeforeResponses,
      rawNoCap,
    }) => {
      setVerboseConfig({
        filters,
        display: FULL_SUBAGENT_DISPLAY,
      });
      const stripped = stripAnsi(
        renderSubagentFinalBlock(
          subagentContent,
          result ?? heavyResult,
          status ?? 'done',
          1234,
          summaries
        )
      );
      expectRender(stripped, { contains, absent });

      const rawIdx = stripped.indexOf('full output:');
      const responsesIdx = stripped.indexOf('response summary:');
      if (requireRawBeforeResponses) {
        expect(rawIdx).toBeGreaterThanOrEqual(0);
        expect(responsesIdx).toBeGreaterThan(rawIdx);
      }
      // `rawContains`/`rawAbsent` here are scoped to the "full output:" slice,
      // not un-stripped ANSI — so assert against the section, not via expectRender.
      const rawSection = section(stripped, 'full output:', 'response summary:');
      for (const c of rawContains) expect(rawSection).toContain(c);
      for (const a of rawAbsent) expect(rawSection).not.toContain(a);
      if (rawNoCap) expect(rawSection).not.toMatch(/\(\+\d+ more lines\)/);
      const responsesSection = stripped.slice(responsesIdx);
      for (const c of responsesContains) expect(responsesSection).toContain(c);
    }
  );
});

// Markdown rendering for subagent outputs is the same pipeline used by the
// parent's `renderAgentMessage`. Scope is strict: only stage body text
// (contextSummary / taskResult) goes through markdown — pipeline tree,
// task line, and error block stay literal.
describe('renderSubagentFinalBlock markdown rendering', () => {
  beforeEach(() => {
    resetVerboseCache();
    delete process.env.KIRO_LITE_VERBOSE;
  });

  afterAll(() => {
    setVerboseConfig({ filters: [] });
    resetVerboseCache();
  });

  const baseContent = JSON.stringify({
    task: '# not a heading\n**not bold**',
    stages: [
      {
        name: 'a',
        role: 'minimal',
        prompt_template: '`backticks` and **bolds** stay literal in prompts.',
      },
    ],
  });
  const okResult = { status: 'success', output: 'irrelevant' };

  const renderWithSummaries = (
    summaries: Parameters<typeof renderSubagentFinalBlock>[4]
  ): string =>
    stripAnsi(
      renderSubagentFinalBlock(baseContent, okResult, 'done', 1000, summaries, {
        display: FULL_SUBAGENT_DISPLAY,
        filtersOverride: ['all'],
      })
    );
  const renderStageBody = (
    contextSummary: string,
    taskResult: string
  ): string =>
    renderWithSummaries([{ stageName: 'a', contextSummary, taskResult }]);

  // Stage body text routes through the SAME markdown pipeline as agent prose;
  // the pipeline's per-syntax mechanics (heading/bold/italic/code/list) are
  // exhaustively covered in render-markdown.test.ts. Here we pin two
  // subagent-distinct facts: (1) contextSummary is rendered through it, and
  // (2) taskResult is the fallback when contextSummary is empty. One compound
  // fixture per slot proves markers strip, bodies survive, and list items pack
  // onto consecutive rows.
  const COMPOUND_MD =
    '# Findings\n\n**bold** and *italic* and `code`.\n\n- alpha\n- beta\n- gamma';
  const COMPOUND_PRESENT = ['Findings', 'bold', 'italic', 'code', '- alpha'];
  const COMPOUND_ABSENT = ['# Findings', '**bold**', '*italic*', '`code`'];
  test.each([
    ['contextSummary', COMPOUND_MD, ''],
    ['taskResult fallback when contextSummary empty', '', COMPOUND_MD],
  ] as const)(
    'stage body markdown via %s',
    (_name, contextSummary, taskResult) => {
      const stripped = renderStageBody(contextSummary, taskResult);
      expectRender(stripped, {
        contains: COMPOUND_PRESENT,
        absent: COMPOUND_ABSENT,
      });
      // List items pack onto consecutive rows (markdown list pipeline, not a
      // blank-separated plain split).
      expect(section(stripped, '▸ a')).toMatch(/- alpha\s*\n\s*- beta/);
    }
  );

  test('prompt_template stays literal while digest bodies render markdown', () => {
    const stripped = renderWithSummaries([
      { stageName: 'a', contextSummary: 'body', taskResult: '' },
    ]);
    expect(stripped).toContain('`backticks`');
    expect(stripped).toContain('**bolds**');
    expect(stripped).toContain('stay literal in prompts.');
  });

  test('strict scope: error block stays literal', () => {
    const errContent = JSON.stringify({ task: 't', stages: [] });
    const errResult = {
      status: 'error',
      error: '# this is the literal error\n**raw** message',
    };
    const block = renderSubagentFinalBlock(
      errContent,
      errResult,
      'error',
      undefined,
      []
    );
    const stripped = stripAnsi(block);
    expect(stripped).toContain('# this is the literal error');
    expect(stripped).toContain('**raw** message');
  });
});

// Migrated from subagent-render.test.ts. These pass `display` explicitly via
// the options arg, so they're independent of the global setVerboseConfig state
// the rest of this file mutates.
describe('renderSubagentFinalBlock — task/cancelled regressions', () => {
  // Stage prompt embeds {task} — the substituted text renders inside the
  // pipeline, so a standalone `task:` line above would duplicate it.
  const withTaskPlaceholder = JSON.stringify({
    task: 'Investigate the flush bug',
    stages: [{ name: 'scan', prompt_template: 'Do this: {task}' }],
  });
  // Prompt does NOT reference {task}; the task: line is still dropped (it
  // duplicates the user's input prompt) and the stage prompt shows verbatim.
  const noPlaceholder = JSON.stringify({
    task: 'Investigate the flush bug',
    stages: [{ name: 'scan', prompt_template: 'Read the static-flush module' }],
  });

  // Bug 3: a terminal 'cancelled' status renders the '✗ cancelled' header
  // suffix, never the running '...'.
  test('Bug 3: cancelled status renders ✗ cancelled, not running ...', () => {
    const header = stripAnsi(
      renderSubagentFinalBlock(
        withTaskPlaceholder,
        { status: 'cancelled' },
        'cancelled',
        undefined,
        undefined,
        { display: FULL_SUBAGENT_DISPLAY }
      )
    ).split('\n')[0]!;
    expect(header).toContain('✗ cancelled');
    expect(header).not.toMatch(/subagent\s*\.\.\.$/);
  });

  // Bug 2: the standalone "task:" key line is always dropped — across the
  // final-block and approval-lines render paths, the placeholder/no-placeholder
  // prompts-hidden preset variants — but the task text still appears when a stage
  // prompt embeds {task}. `render` carries the path so both share the assertion.
  const finalBlock = (content: string, display: typeof DEFAULT_DISPLAY) =>
    stripAnsi(
      renderSubagentFinalBlock(
        content,
        undefined,
        'running',
        undefined,
        undefined,
        {
          display,
        }
      )
    );
  const approvalLines = (content: string) =>
    stripAnsi(formatSubagentApprovalLines(content, 80)!.join('\n'));
  test.each<{
    name: string;
    render: () => string;
    taskTextShown: boolean;
  }>([
    {
      name: 'final block, prompts on, {task} embedded',
      render: () => finalBlock(withTaskPlaceholder, FULL_SUBAGENT_DISPLAY),
      taskTextShown: true,
    },
    {
      name: 'final block, prompts on, no {task}',
      render: () => finalBlock(noPlaceholder, FULL_SUBAGENT_DISPLAY),
      taskTextShown: false,
    },
    {
      name: 'final block, lean preset (prompts hidden)',
      render: () => finalBlock(withTaskPlaceholder, DENSITY_DISPLAY.lean),
      taskTextShown: false,
    },
    {
      name: 'approval lines, {task} embedded',
      render: () => approvalLines(withTaskPlaceholder),
      taskTextShown: true,
    },
    {
      name: 'approval lines, no {task}',
      render: () => approvalLines(noPlaceholder),
      taskTextShown: false,
    },
  ])(
    'Bug 2: omits standalone task: line ($name)',
    ({ render, taskTextShown }) => {
      const out = render();
      expect(out).not.toMatch(/^\s*task:/m);
      if (taskTextShown) expect(out).toContain('Investigate the flush bug');
    }
  );
});
