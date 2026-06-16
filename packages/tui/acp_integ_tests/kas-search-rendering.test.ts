/**
 * ACP integ test: KAS file_search / grep_search render the query arg as the
 * pattern target instead of dumping it into ToolMeta.
 *
 * KAS sends these tools with `{ query, explanation }` rather than `{ pattern }`.
 * The Glob and Grep components must read `query` as a fallback for `pattern`.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
  PromptRequest,
  PromptResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const SESSION_ID = 'search-session-1';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: SESSION_ID,
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

async function emitSearchTool(
  tc: AcpTestCase,
  tool: { id: string; title: string; kind: string; rawInput: unknown },
  output: string | Record<string, unknown>,
  finalText: string
): Promise<void> {
  // String output is the KAS text envelope (wrapped in `message`); an object is
  // the V2/legacy structured rawOutput, sent verbatim.
  const rawOutput = typeof output === 'string' ? { message: output } : output;
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'tool_call',
      toolCallId: tool.id,
      title: tool.title,
      kind: tool.kind,
      rawInput: tool.rawInput,
    },
  });
  await new Promise((r) => setTimeout(r, 200));
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'tool_call_update',
      toolCallId: tool.id,
      status: 'completed',
      rawOutput,
    },
  });
  await new Promise((r) => setTimeout(r, 200));
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'agent_message_chunk',
      content: { type: 'text', text: finalText },
    },
  });
  await new Promise((r) => setTimeout(r, 100));
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'session_info_update',
      _meta: { kiro: { kind: 'turn_completion' } },
    },
  });
}

describe('KAS search tool rendering', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('file_search (glob) renders query as pattern target', async () => {
    tc = new AcpTestCase({ testName: 'kas-file-search-query' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'glob-1',
          title: 'file_search',
          kind: 'search',
          rawInput: {
            query: 'vitest.config',
            explanation: 'Find the Vitest config file',
          },
        },
        'You searched for vitest.config and received the following complete results:\n---\nvitest.config.ts\n---',
        'Found the config.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('find vitest config');
    await tc.pressEnter();
    await tc.waitForVisibleText('Found the config', 10000);

    // The query should render as the target (quoted), not dumped as ToolMeta.
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('"vitest.config"');
    // The explanation should NOT appear as raw meta text.
    expect(snapshot).not.toContain('explanation=');
    // The raw text envelope should NOT be shown; the file list should render.
    expect(snapshot).not.toContain('You searched for');
    expect(snapshot).toContain('vitest.config.ts');
  });

  it('grep_search renders query as pattern target', async () => {
    tc = new AcpTestCase({ testName: 'kas-grep-search-query' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'grep-1',
          title: 'grep_search',
          kind: 'search',
          rawInput: {
            query: 'TODO',
            explanation: 'Find all TODO comments',
          },
        },
        'You searched for TODO and received the following results:\nsrc/index.ts\n5:// TODO: refactor\n6-const x = 1;\nsrc/utils.ts\n12:// TODO: add tests\nsrc/main.ts\n8:// TODO: cleanup',
        'Found TODOs.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('find TODOs');
    await tc.pressEnter();
    await tc.waitForVisibleText('Found TODOs', 10000);

    // The query should render as the pattern target.
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('"TODO"');
    expect(snapshot).not.toContain('explanation=');
    // The raw text envelope should NOT be shown.
    expect(snapshot).not.toContain('You searched for');
    // Parsed into 3 matches across 3 files (context line not counted).
    expect(snapshot).toContain('3 matches in 3 files');
  });

  it('file_search incomplete results still render the file list', async () => {
    tc = new AcpTestCase({ testName: 'kas-file-search-incomplete' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'glob-2',
          title: 'file_search',
          kind: 'search',
          rawInput: { query: 'src/**/*.tsx', explanation: 'Find components' },
        },
        // Incomplete results carry a trailing "Refine your search…" message
        // after the second --- delimiter; the file list must still render.
        'You searched for src/**/*.tsx and received the following incomplete results:\n---\nsrc/a.tsx\nsrc/b.tsx\nsrc/c.tsx\nsrc/d.tsx\n---\nRefine your search, or use the excludePattern to retrieve all results.',
        'Listed components.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('list components');
    await tc.pressEnter();
    await tc.waitForVisibleText('Listed components', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('"src/**/*.tsx"');
    // The file list must render even though there's a trailing message.
    // FileList shows basenames in the preview.
    expect(snapshot).toContain('a.tsx');
    expect(snapshot).toContain('4 files');
    // The raw envelope/trailer should not be shown verbatim.
    expect(snapshot).not.toContain('You searched for');
    expect(snapshot).not.toContain('use the excludePattern');
  });

  it('grep_search with many matches in one file is expandable', async () => {
    tc = new AcpTestCase({ testName: 'kas-grep-many-in-one-file' });
    setupHandshake(tc);
    // One file with 6 matches — more than PREVIEW_MATCHES_PER_FILE (3), so the
    // per-file matches overflow even though there's only a single file. ctrl+o
    // must still be offered (regression: it was keyed on file count only).
    const matchLines = Array.from(
      { length: 6 },
      (_, i) => `${i + 1}:export const x${i} = ${i};`
    ).join('\n');
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'grep-2',
          title: 'grep_search',
          kind: 'search',
          rawInput: { query: 'export', explanation: 'Find exports' },
        },
        `You searched for export and received the following results:\nsrc/consts.ts\n${matchLines}`,
        'Found exports.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('find exports');
    await tc.pressEnter();
    await tc.waitForVisibleText('Found exports', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('6 matches in 1 file');
    // ctrl+o affordance must appear because matches overflow the per-file preview.
    expect(snapshot).toContain('ctrl+o');
    expect(snapshot).not.toContain('You searched for');
  });

  it('V2 structured no-results (glob) renders no matches, not a fake file', async () => {
    tc = new AcpTestCase({ testName: 'kas-glob-v2-no-results' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'glob-3',
          title: 'file_search',
          kind: 'search',
          rawInput: { query: '**/*.xyz', explanation: 'Find xyz files' },
        },
        // V2/legacy structured shape: the `message` is the empty-case label and
        // must NOT be rendered as a file entry.
        {
          filePaths: [],
          totalFiles: 0,
          message: 'No files found matching pattern: **/*.xyz',
        },
        'No matches.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('find xyz');
    await tc.pressEnter();
    await tc.waitForVisibleText('No matches', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('No files found matching pattern');
    // Must not be counted/rendered as a file.
    expect(snapshot).not.toContain('1 file');
  });

  it('V2 structured no-results (grep) renders no matches, not a fake file', async () => {
    tc = new AcpTestCase({ testName: 'kas-grep-v2-no-results' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'grep-3',
          title: 'grep_search',
          kind: 'search',
          rawInput: { query: 'nonexistent', explanation: 'Find nothing' },
        },
        { numMatches: 0, numFiles: 0 },
        'No matches.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('find nothing');
    await tc.pressEnter();
    await tc.waitForVisibleText('No matches', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('no matches');
    expect(snapshot).not.toContain('1 file');
  });

  it('grep_search with a colon in the query strips the full header', async () => {
    tc = new AcpTestCase({ testName: 'kas-grep-colon-query' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'grep-4',
          title: 'grep_search',
          kind: 'search',
          rawInput: { query: 'TODO:', explanation: 'Find TODO comments' },
        },
        // The query "TODO:" contains a colon; a lazy header strip stops at the
        // first colon and leaks "… results:" as a fake file. The header must be
        // removed in full so only the two real files are parsed.
        'You searched for TODO: and received the following results:\nsrc/index.ts\n5:// TODO: refactor\nsrc/utils.ts\n12:// TODO: add tests',
        'Found TODOs.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('find TODO colon');
    await tc.pressEnter();
    await tc.waitForVisibleText('Found TODOs', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('2 matches in 2 files');
    // The header fragment must not survive as a file header.
    expect(snapshot).not.toContain('received the following results');
    expect(snapshot).not.toContain('You searched for');
  });

  it('file_search no-delimiter no-match text renders no matches', async () => {
    tc = new AcpTestCase({ testName: 'kas-glob-no-delim-no-match' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'glob-4',
          title: 'file_search',
          kind: 'search',
          rawInput: { query: 'zzz', explanation: 'Find zzz' },
        },
        // Text envelope with no --- delimiters and a no-match body.
        'You searched for zzz and received the following results:\nNo matches found.',
        'No matches.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('find zzz');
    await tc.pressEnter();
    await tc.waitForVisibleText('No matches', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('no matches');
    expect(snapshot).not.toContain('You searched for');
  });

  it('V2 structured glob output (filePaths) still renders the file list', async () => {
    tc = new AcpTestCase({ testName: 'v2-glob-structured' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'glob-v2',
          title: 'glob',
          kind: 'search',
          rawInput: { pattern: 'src/**/*.ts' },
        },
        // V2/legacy structured rawOutput (no `message`, no text envelope).
        {
          filePaths: ['src/index.ts', 'src/utils.ts'],
          totalFiles: 2,
          truncated: false,
        },
        'Listed files.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('list ts files');
    await tc.pressEnter();
    await tc.waitForVisibleText('Listed files', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('"src/**/*.ts"');
    expect(snapshot).toContain('2 files');
    expect(snapshot).toContain('index.ts');
    expect(snapshot).not.toContain('You searched for');
  });

  it('V2 structured grep output (results) still renders matches', async () => {
    tc = new AcpTestCase({ testName: 'v2-grep-structured' });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      await emitSearchTool(
        tc!,
        {
          id: 'grep-v2',
          title: 'grep',
          kind: 'search',
          rawInput: { pattern: 'foo' },
        },
        // V2/legacy structured rawOutput.
        {
          numMatches: 2,
          numFiles: 1,
          truncated: false,
          results: [
            { file: 'src/a.ts', count: 2, matches: ['1:foo()', '5:foo bar'] },
          ],
        },
        'Found matches.'
      );
      return { stopReason: 'end_turn' } as unknown as PromptResponse;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sendKeys('grep foo');
    await tc.pressEnter();
    await tc.waitForVisibleText('Found matches', 10000);

    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('"foo"');
    expect(snapshot).toContain('2 matches in 1 file');
    expect(snapshot).toContain('a.ts');
    expect(snapshot).not.toContain('You searched for');
  });
});
