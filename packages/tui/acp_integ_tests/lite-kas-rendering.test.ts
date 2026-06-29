/**
 * ACP integ test: KAS agent engine rendering + command seeding on the LITE UI.
 *
 * Ported from the former `integ_tests/lite-kas-engine.test.ts`, which booted the
 * `MockSessionClient` harness with `KIRO_AGENT_ENGINE=kas`. Per review on PR
 * #3259, KAS coverage must live in `acp_integ_tests`: `integ_tests` targets the
 * V2 engine (being phased out), and the `AcpTestCase` harness is the KAS-only
 * mock-transport path. These run the real `KasAcpClient` against the mock ACP
 * wire with `KIRO_UI_MODE=lite`, so lite's own render paths (`src/lite/render.ts`)
 * are exercised end to end — a strictly wider path than the old mock-session
 * version, since the real client's wire parsing is now in the loop too.
 *
 * Companion: `lite-kas-consent.test.ts` covers the lite consent keyboard model.
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
import { MessageRole } from '../src/stores/app-store';
import type { SerializedAppState } from '../src/test-utils/shared/ipc-types';

const SESSION_ID = 'lite-kas-session';

/**
 * Boot the real KasAcpClient under the lite UI. `argsMode: 'inline'` pins the
 * lite verbose config to a sandboxed cli.json so the tool-arg chip assertions
 * are deterministic regardless of the developer's real ~/.kiro lite_verbose.json
 * (the default 'block' mode dumps a key:value tree instead of inline chips).
 */
function liteKas(testName: string): AcpTestCase {
  return new AcpTestCase({
    testName,
    extraEnv: { KIRO_UI_MODE: 'lite', KIRO_LITE_ROLLOUT_ENABLED: '1' },
    settings: { 'chat.tools.argsMode': 'inline' },
  });
}

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

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

function notify(tc: AcpTestCase, update: Record<string, unknown>): void {
  tc.mock.notify('session/update', { sessionId: SESSION_ID, update });
}

/** Boot lite+KAS with a no-op prompt handler; for tests that only inspect boot state. */
async function bootLiteKas(tc: AcpTestCase): Promise<void> {
  setupHandshake(tc);
  tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));
  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForVisibleText('ask a question', 10000);
}

/**
 * Boot lite+KAS and register a `session/prompt` handler that emits `emit()` (the
 * tool sequence) when the user submits a turn. Mirrors `kas-search-rendering`.
 */
async function bootLiteKasWithTurn(
  tc: AcpTestCase,
  emit: () => Promise<void>
): Promise<void> {
  setupHandshake(tc);
  tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
    await emit();
    return { stopReason: 'end_turn' } as unknown as PromptResponse;
  });
  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForVisibleText('ask a question', 10000);
}

function toolUseNames(store: SerializedAppState): string[] {
  return store.messages
    .filter((m) => m.role === MessageRole.ToolUse)
    .map((m) => ('name' in m ? (m.name ?? '') : ''));
}

describe('KAS engine on lite UI (wire)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('boots lite under the KAS engine and seeds the KAS command slice', async () => {
    tc = liteKas('lite-kas-boot');
    await bootLiteKas(tc);

    const store = await tc.getStore();
    expect(store.agentEngine).toBe('kas');
    expect(store.uiMode).toBe('lite');

    // The KAS command slice is seeded only under the KAS engine (app-store.ts);
    // v2 leaves it empty (covered by app-store.kas-pipeline.test.ts). /spec,
    // /plan and /compact are KAS-only and must reach the lite store.
    const kasNames = store.kasCommands.map((c) => String(c.name));
    expect(kasNames).toContain('/spec');
    expect(kasNames).toContain('/plan');
    expect(kasNames).toContain('/compact');
  }, 40000);

  it('surfaces the KAS-only /spec command in the lite slash dropdown', async () => {
    tc = liteKas('lite-kas-spec-menu');
    await bootLiteKas(tc);

    // Per-char so the autocomplete dropdown opens (a single batched write can
    // submit before the menu renders).
    for (const ch of '/spec') {
      await tc.sendKeys(ch);
      await tc.sleepMs(40);
    }

    // selectVisibleSlashCommands prepends kasCommands under KAS, so the dropdown
    // renders /spec with its description — proof the seed reaches the lite menu.
    // The description is the discriminator (the typed "/spec" echoes regardless).
    await tc.waitForVisibleText('switch to spec mode', 3000);
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).toContain('/spec');
    expect(snapshot).toContain('switch to spec mode');
  }, 40000);

  it('renders KAS read/edit/create tool titles as clean labelled chips without leaking raw payloads', async () => {
    tc = liteKas('lite-kas-tool-rendering');
    await bootLiteKasWithTurn(tc!, async () => {
      // KAS sends tool TITLES as the name and arg shapes that differ from the v2
      // wire tools: read_file is flat {path,offset,limit}, fs_write is
      // {path,text}, str_replace is {path,oldStr,newStr}. Lite must map them to
      // friendly labels (Read/Write) and never dump the raw result envelope.
      notify(tc!, {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-read',
        title: 'Read File',
        kind: 'read',
        rawInput: { path: 'src/index.ts', offset: 9, limit: 20 },
      });
      await wait(150);
      notify(tc!, {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-read',
        status: 'completed',
        rawOutput: { message: '<file>secret body</file>' },
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-replace',
        title: 'Replace in File',
        kind: 'edit',
        rawInput: {
          path: 'src/app.ts',
          oldStr: 'const x = 1;\n',
          newStr: 'const x = 10;\n',
        },
      });
      await wait(150);
      notify(tc!, {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-replace',
        status: 'completed',
        rawOutput: { ok: true },
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-write',
        title: 'Write File',
        kind: 'edit',
        rawInput: {
          path: 'src/new-file.ts',
          text: 'export const hello = 1;\n',
        },
      });
      await wait(150);
      notify(tc!, {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-write',
        status: 'completed',
        rawOutput: { ok: true },
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'KAS_TOOLS_DONE' },
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind: 'turn_completion' } },
      });
    });

    await tc.sendKeys('go');
    await tc.pressEnter();
    // Render barrier: the turn must fully settle (rows flushed from the live
    // region to <Static>) AND the trailing content must paint before we read.
    await tc.waitForStore((s) => !s.isProcessing, 10000);
    await tc.waitForVisibleText('KAS_TOOLS_DONE', 10000);
    const flat = tc.getSnapshotFormatted();

    // read_file → friendly "Read" + path chip (inline args mode); the raw result
    // envelope must never reach the UI.
    expect(flat).toContain('Read [src/index.ts');
    expect(flat).not.toContain('secret body');
    expect(flat).not.toContain('"message"');

    // str_replace → "Write" with the "edit" verb inferred from the oldStr/newStr
    // shape; the KAS {path,text} create resolves the verb to "create".
    expect(flat).toContain('Write [edit src/app.ts]');
    expect(flat).toContain('Write [create src/new-file.ts]');

    // Store keeps the KAS wire titles verbatim (acp-client maps title→name);
    // the friendly labels are a pure render-time transform.
    expect(toolUseNames(await tc.getStore())).toEqual([
      'Read File',
      'Replace in File',
      'Write File',
    ]);
  }, 40000);

  it('renders a failed KAS read with a friendly Read label and the error, not the raw tool id', async () => {
    tc = liteKas('lite-kas-failed-read');
    await bootLiteKasWithTurn(tc!, async () => {
      notify(tc!, {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-read-err',
        title: 'read_files',
        kind: 'read',
        rawInput: { paths: ['packages/tui/TESTING.md'] },
      });
      await wait(150);
      // acp-client derives the failed-tool error from a text content block
      // (preferred over rawOutput); lite paints it in the red error bar.
      notify(tc!, {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-read-err',
        status: 'failed',
        content: [
          {
            type: 'content',
            content: {
              type: 'text',
              text: 'ENOENT: no such file or directory',
            },
          },
        ],
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'RETRYING_NOW' },
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind: 'turn_completion' } },
      });
    });

    await tc.sendKeys('go');
    await tc.pressEnter();
    await tc.waitForStore((s) => !s.isProcessing, 10000);
    await tc.waitForVisibleText('RETRYING_NOW', 10000);
    const flat = tc.getSnapshotFormatted();
    // Friendly "Read" label on the failed row + surfaced error, never the raw
    // wire id (read_files is lowercase, so a bare "Read" match wouldn't catch a
    // regression that leaked the id — assert the FAILED state + error directly).
    expect(flat).toContain('Read');
    expect(flat).toContain('FAILED');
    expect(flat).toContain('ENOENT: no such file or directory');
    expect(flat).not.toContain('read_files');
  }, 40000);

  it('renders KAS search tools with the query as the chip target, not the explanation or envelope', async () => {
    tc = liteKas('lite-kas-search');
    await bootLiteKasWithTurn(tc!, async () => {
      // KAS search tools carry {query, explanation} (not v2's {pattern}) and a
      // text-envelope result. Lite's inline-arg chip reads `query`; neither the
      // explanation nor the "You searched for…" envelope may leak.
      notify(tc!, {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-grep',
        title: 'grep_search',
        kind: 'search',
        rawInput: { query: 'TODO', explanation: 'Find all TODO comments' },
      });
      await wait(150);
      notify(tc!, {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-grep',
        status: 'completed',
        rawOutput: {
          message:
            'You searched for TODO and received the following results:\nsrc/a.ts\n5:// TODO: x',
        },
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-glob',
        title: 'file_search',
        kind: 'search',
        rawInput: { query: 'vitest.config', explanation: 'Find the config' },
      });
      await wait(150);
      notify(tc!, {
        sessionUpdate: 'tool_call_update',
        toolCallId: 'tc-glob',
        status: 'completed',
        rawOutput: { message: '---\nvitest.config.ts\n---' },
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'agent_message_chunk',
        content: { type: 'text', text: 'SEARCH_DONE' },
      });
      await wait(100);
      notify(tc!, {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind: 'turn_completion' } },
      });
    });

    await tc.sendKeys('go');
    await tc.pressEnter();
    await tc.waitForStore((s) => !s.isProcessing, 10000);
    await tc.waitForVisibleText('SEARCH_DONE', 10000);
    const flat = tc.getSnapshotFormatted();
    expect(flat).toContain('Grep [TODO]');
    expect(flat).toContain('Glob [vitest.config]');
    // The explanation arg is metadata for the model, not the user — its text and
    // the raw result envelope must stay out of the chip/scrollback.
    expect(flat).not.toContain('Find all TODO comments');
    expect(flat).not.toContain('You searched for');
  }, 40000);
});
