/**
 * Lite approval-prompt "attach notes" tests (staged-note flow).
 *
 * Lite mode lets the user attach free-text feedback to a tool approval without
 * losing the disposition they pick. The flow (see ApprovalPrompt.tsx):
 *   - [tab] swaps the y/t/n hotkey row for a feedback PromptInput;
 *   - submitting that input STAGES the text and returns to the picker — it does
 *     NOT resolve or cancel the approval (the old behavior force-denied every
 *     noted request);
 *   - the staged note rides along as a follow-up user turn only AFTER the user
 *     picks y/t/n, via respondWithNote → respondToApproval(...) then
 *     onNotesSubmit(stagedNote).
 *
 * The backend approval response can't carry free text (v2 ApprovalResult.reason
 * is ignored), so the note must reach the model as a new user turn — but only
 * after the tool gets the disposition the user actually chose.
 *
 * These tests cover:
 *   - the fast path (y / t / n with no staged note) still calls
 *     respondToApproval and does NOT inject a turn;
 *   - Tab from the default page swaps the hotkey row for the feedback input;
 *   - submitting the feedback input STAGES the note (returns to the picker,
 *     does not resolve/inject);
 *   - staging a note then picking [y] sends the disposition AND flushes the
 *     note as a follow-up turn — the core "keep the disposition" guarantee;
 *   - Esc from the notes page steps back to the default page without injecting;
 *   - the LiteLayout closure injects the staged note as a new turn (and does
 *     NOT cancel the approval, since the disposition was already sent).
 */

import { describe, test, expect, afterEach, vi } from 'vitest';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  type MessageType,
} from '../../../../stores/app-store.js';
import { Kiro } from '../../../../kiro.js';
import { ApprovalOptionId } from '../../../../types/agent-events.js';
import { ApprovalPrompt } from '../ApprovalPrompt.js';

// Raw byte sequences twinki decodes back into Key events (see
// hooks/useKeypress.ts keyToRawBytes + twinki input/keys.ts).
const TAB = '\t';
const ESC = '\x1b';
const ENTER = '\r';

class MockTerminal implements Terminal {
  private _onInput: ((data: string) => void) | null = null;
  public output = '';
  get columns() {
    return 80;
  }
  get rows() {
    return 24;
  }
  get kittyProtocolActive() {
    return true;
  }
  start(onInput: (data: string) => void): void {
    this._onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
  }
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
  sendInput(data: string): void {
    if (this._onInput) this._onInput(data);
  }
}

let activeInstance: Instance | null = null;
afterEach(() => {
  if (activeInstance) {
    activeInstance.unmount();
    activeInstance = null;
  }
  vi.useRealTimers();
});

/**
 * Settle render + effects so twinki's useInput subscription is live before we
 * send keys. twinki registers that subscription on a MACROTASK turn after the
 * first commit (terminal.start() runs earlier, so the handler being wired is
 * NOT a sufficient signal — the React effect hasn't subscribed yet). One timer
 * turn left the first test flaky (~17/20 under load); empirically two turns is
 * reliable (20/20), so pump several timer turns for CI headroom. Microtask
 * drains alone never advance it — the subscription only runs on a timer turn.
 */
async function flush(): Promise<void> {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

// A minimal approval shape — only the fields ApprovalPrompt reads. The tool
// message is matched by toolCallId so the header/detail render path runs.
function makeApproval() {
  return {
    toolCall: { toolCallId: 'call-1', title: 'execute_bash', rawInput: '' },
    permissionOptions: [
      { kind: ApprovalOptionId.AllowOnce, optionId: 'allow_once' },
      { kind: ApprovalOptionId.RejectOnce, optionId: 'reject_once' },
      { kind: ApprovalOptionId.AllowAlways, optionId: 'allow_always' },
    ],
    trustOptions: [],
  };
}

const TOOL_MSG: MessageType = {
  id: 'call-1',
  role: MessageRole.ToolUse,
  name: 'execute_bash',
  content: JSON.stringify({ command: 'ls -la' }),
  isFinished: false,
};

interface Harness {
  terminal: MockTerminal;
  respondToApproval: ReturnType<typeof vi.fn>;
  onNotesSubmit: ReturnType<typeof vi.fn>;
}

// Default store pins v2 so baseline tests don't pick up a leaked
// KIRO_AGENT_ENGINE=kas from another suite (which would flip [t] to the KAS
// whole-capability path). KAS tests pass their own agentEngine:'kas' store.
function mountApproval(
  store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' })
): Harness {
  const terminal = new MockTerminal();
  const respondToApproval = vi.fn();
  const onNotesSubmit = vi.fn();

  const instance = render(
    <AppStoreContext.Provider value={store}>
      <ApprovalPrompt
        messages={[TOOL_MSG]}
        approval={makeApproval()}
        respondToApproval={respondToApproval}
        getStageInputColor={() => (t: string) => t}
        mainAgentName="main"
        onNotesSubmit={onNotesSubmit}
      />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  activeInstance = instance;
  return { terminal, respondToApproval, onNotesSubmit };
}

function mountApprovalPainter(toolMsg: MessageType): Harness {
  if (toolMsg.role !== MessageRole.ToolUse) {
    throw new Error('Expected a tool-use message');
  }

  const terminal = new MockTerminal();
  const respondToApproval = vi.fn();
  const onNotesSubmit = vi.fn();
  const rawInput = JSON.parse(toolMsg.content);
  const instance = render(
    <AppStoreContext.Provider
      value={createAppStore({ kiro: new Kiro(), agentEngine: 'v2' })}
    >
      <ApprovalPrompt
        messages={[toolMsg]}
        approval={{
          toolCall: {
            toolCallId: toolMsg.id,
            title: toolMsg.originalTitle ?? toolMsg.name,
            rawInput,
          },
          permissionOptions: [
            { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
            { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
          ],
          trustOptions: [],
        }}
        respondToApproval={respondToApproval}
        getStageInputColor={() => (text: string) => text}
        mainAgentName="main"
        onNotesSubmit={onNotesSubmit}
      />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  activeInstance = instance;
  return { terminal, respondToApproval, onNotesSubmit };
}

describe('ApprovalPrompt — fast path (no staged note)', () => {
  // respondWithNote calls respondToApproval(optionId, undefined, meta); with no
  // trust submenu the meta arg is undefined, so the call carries three args.
  test('[y] allows once and injects no turn', async () => {
    const h = mountApproval();
    await flush();
    h.terminal.sendInput('y');
    await flush();
    expect(h.respondToApproval).toHaveBeenCalledWith(
      'allow_once',
      undefined,
      undefined
    );
    expect(h.onNotesSubmit).not.toHaveBeenCalled();
  });

  test('[n] denies once and injects no turn', async () => {
    const h = mountApproval();
    await flush();
    h.terminal.sendInput('n');
    await flush();
    expect(h.respondToApproval).toHaveBeenCalledWith(
      'reject_once',
      undefined,
      undefined
    );
    expect(h.onNotesSubmit).not.toHaveBeenCalled();
  });

  test('[t] with no trust tiers trusts the whole tool directly', async () => {
    const h = mountApproval();
    await flush();
    h.terminal.sendInput('t');
    await flush();
    expect(h.respondToApproval).toHaveBeenCalledWith(
      'allow_always',
      undefined,
      undefined
    );
    expect(h.onNotesSubmit).not.toHaveBeenCalled();
  });
});

describe('ApprovalPrompt — tool display label', () => {
  // KAS ships the raw title 'Run Command'; the header must show the v2 display
  // label 'Shell' to match scrollback (toolDisplayName), not the raw wire name.
  test('renders the display label (Run Command -> Shell) in the header', async () => {
    const terminal = new MockTerminal();
    const toolMsg: MessageType = {
      id: 'call-1',
      role: MessageRole.ToolUse,
      name: 'Run Command',
      content: JSON.stringify({ command: 'git status' }),
      isFinished: false,
    };
    const instance = render(
      <AppStoreContext.Provider
        value={createAppStore({ kiro: new Kiro(), agentEngine: 'kas' })}
      >
        <ApprovalPrompt
          messages={[toolMsg]}
          approval={{
            toolCall: {
              toolCallId: 'call-1',
              title: 'Run Command',
              rawInput: '',
            },
            permissionOptions: [
              { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
              { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
            ],
            trustOptions: [],
          }}
          respondToApproval={vi.fn()}
          getStageInputColor={() => (t: string) => t}
          mainAgentName="main"
          onNotesSubmit={vi.fn()}
        />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    activeInstance = instance;
    await flush();
    expect(terminal.output).toContain('Shell');
    expect(terminal.output).not.toContain('Run Command');
  });
});

describe('ApprovalPrompt — tool payload rendering', () => {
  test('renders an eligible edit-kind tool as a write diff', async () => {
    const h = mountApprovalPainter({
      id: 'call-write',
      role: MessageRole.ToolUse,
      name: 'Patch Workspace',
      kind: 'edit',
      content: JSON.stringify({
        command: 'str_replace',
        path: '/workspace/file.ts',
        old_str: 'const value = 1;',
        new_str: 'const value = 2;',
      }),
      isFinished: false,
    });
    await flush();
    const output = stripAnsi(h.terminal.output);

    expect(output).toContain('Write');
    expect(output).toContain('/workspace/file.ts');
    expect(output).toMatch(/-\s+const value = 1;/);
    expect(output).toMatch(/\+\s+const value = 2;/);
    expect(output).not.toContain('old_str:');
    expect(output).not.toContain('new_str:');
  });

  test('renders a parent subagent with the dedicated pipeline painter', async () => {
    const h = mountApprovalPainter({
      id: 'call-subagent',
      role: MessageRole.ToolUse,
      name: 'orchestrate_subagent',
      content: JSON.stringify({
        task: 'Audit approval routing',
        stages: [
          {
            name: 'reviewer',
            role: 'explorer',
            prompt_template: 'Inspect the approval components',
          },
        ],
      }),
      isFinished: false,
    });
    await flush();
    const output = stripAnsi(h.terminal.output);

    expect(output).toContain('pipeline:');
    expect(output).toContain('[reviewer]');
    expect(output).toContain('(explorer)');
    expect(output).toContain('Inspect the approval components');
    expect(output).not.toContain('stages:');
  });

  test('renders ordinary tool arguments without specialized paint', async () => {
    const h = mountApprovalPainter({
      id: 'call-generic',
      role: MessageRole.ToolUse,
      name: 'custom_inspector',
      content: JSON.stringify({
        query: 'approval routing',
        limit: 2,
      }),
      isFinished: false,
    });
    await flush();
    const output = stripAnsi(h.terminal.output);

    expect(output).toContain('custom_inspector');
    expect(output).toContain('query:');
    expect(output).toContain('approval routing');
    expect(output).toContain('limit:');
    expect(output).not.toContain('pipeline:');
  });

  test('keeps an MCP edit-kind name collision generic and renders arguments, not a diff', async () => {
    const h = mountApprovalPainter({
      id: 'call-1',
      role: MessageRole.ToolUse,
      name: 'fs_write',
      origin: 'mcp',
      originalTitle: '@server/fs_write',
      kind: 'edit',
      content: JSON.stringify({
        command: 'str_replace',
        path: '/workspace/file.ts',
        old_str: 'before',
        new_str: 'after',
      }),
      isFinished: false,
    });
    await flush();
    const output = stripAnsi(h.terminal.output);

    expect(output).toContain('fs_write');
    expect(output).toContain('command:');
    expect(output).toContain('old_str:');
    expect(output).not.toContain('Write needs approval');
    expect(output).not.toContain('added 1 line');
    expect(output).not.toMatch(/-\s+before/);
    expect(output).not.toMatch(/\+\s+after/);
  });

  test('renders MCP permission arguments before the ToolUse message arrives', async () => {
    const terminal = new MockTerminal();
    const instance = render(
      <AppStoreContext.Provider
        value={createAppStore({ kiro: new Kiro(), agentEngine: 'v2' })}
      >
        <ApprovalPrompt
          messages={[]}
          approval={{
            toolCall: {
              toolCallId: 'call-mcp-first',
              title: '@weather/get_forecast',
              rawInput: { city: 'Seattle', units: 'metric' },
              name: 'get_forecast',
              origin: 'mcp',
            },
            permissionOptions: [
              { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
              { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
            ],
            trustOptions: [],
          }}
          respondToApproval={vi.fn()}
          getStageInputColor={() => (text: string) => text}
          mainAgentName="main"
          onNotesSubmit={vi.fn()}
        />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    activeInstance = instance;
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('get_forecast');
    expect(output).toContain('city:');
    expect(output).toContain('Seattle');
    expect(output).toContain('units:');
    expect(output).toContain('metric');
  });

  test('renders a permission-first built-in write as a diff', async () => {
    const terminal = new MockTerminal();
    const instance = render(
      <AppStoreContext.Provider
        value={createAppStore({ kiro: new Kiro(), agentEngine: 'kas' })}
      >
        <ApprovalPrompt
          messages={[]}
          approval={{
            toolId: 'fs_write',
            toolCall: {
              toolCallId: 'call-write-first',
              title: 'Creating report.ts',
              name: 'fs_write',
              kind: 'edit',
              origin: 'builtin',
              rawInput: {
                command: 'str_replace',
                path: '/workspace/report.ts',
                old_str: 'export const ready = false;',
                new_str: 'export const ready = true;',
              },
            },
            permissionOptions: [
              { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
              { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
            ],
            trustOptions: [],
          }}
          respondToApproval={vi.fn()}
          getStageInputColor={() => (text: string) => text}
          mainAgentName="main"
          onNotesSubmit={vi.fn()}
        />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    activeInstance = instance;
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('Write needs approval');
    expect(output).toContain('/workspace/report.ts');
    expect(output).toMatch(/-\s+export const ready = false;/);
    expect(output).toMatch(/\+\s+export const ready = true;/);
    expect(output).not.toContain('old_str:');
  });

  test('renders permission arguments while the ToolUse message is an empty chunk placeholder', async () => {
    const terminal = new MockTerminal();
    const instance = render(
      <AppStoreContext.Provider
        value={createAppStore({ kiro: new Kiro(), agentEngine: 'v2' })}
      >
        <ApprovalPrompt
          messages={[
            {
              id: 'call-mcp-chunk-first',
              role: MessageRole.ToolUse,
              name: 'get_forecast',
              origin: 'mcp',
              originalTitle: '@weather/get_forecast',
              content: '{}',
              isFinished: false,
            },
          ]}
          approval={{
            toolCall: {
              toolCallId: 'call-mcp-chunk-first',
              title: '@weather/get_forecast',
              rawInput: { city: 'Seattle', units: 'metric' },
            },
            permissionOptions: [
              { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
              { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
            ],
            trustOptions: [],
          }}
          respondToApproval={vi.fn()}
          getStageInputColor={() => (text: string) => text}
          mainAgentName="main"
          onNotesSubmit={vi.fn()}
        />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    activeInstance = instance;
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('city:');
    expect(output).toContain('Seattle');
    expect(output).toContain('units:');
    expect(output).toContain('metric');
  });

  test('renders edit permission input instead of the synthesized empty edit placeholder', async () => {
    const terminal = new MockTerminal();
    const instance = render(
      <AppStoreContext.Provider
        value={createAppStore({ kiro: new Kiro(), agentEngine: 'v2' })}
      >
        <ApprovalPrompt
          messages={[
            {
              id: 'call-edit-chunk-first',
              role: MessageRole.ToolUse,
              name: 'fs_write',
              kind: 'edit',
              content: JSON.stringify({ command: 'create', content: '' }),
              isFinished: false,
            },
          ]}
          approval={{
            toolCall: {
              toolCallId: 'call-edit-chunk-first',
              title: 'Creating report.ts',
              rawInput: {
                command: 'create',
                path: '/workspace/report.ts',
                content: 'export const ready = true;',
              },
            },
            permissionOptions: [
              { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
              { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
            ],
            trustOptions: [],
          }}
          respondToApproval={vi.fn()}
          getStageInputColor={() => (text: string) => text}
          mainAgentName="main"
          onNotesSubmit={vi.fn()}
        />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false }
    );
    activeInstance = instance;
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('/workspace/report.ts');
    expect(output).toContain('export const ready = true;');
  });
});

describe('ApprovalPrompt — staged-note flow', () => {
  test('default page advertises [tab] add note', async () => {
    const h = mountApproval();
    await flush();
    expect(h.terminal.output).toContain('add note');
    // The feedback input should NOT be mounted yet.
    expect(h.terminal.output).not.toContain('add your feedback');
  });

  test('Tab swaps the hotkey row for the feedback input', async () => {
    const h = mountApproval();
    await flush();
    h.terminal.output = '';
    h.terminal.sendInput(TAB);
    await flush();
    // PromptInput renders its placeholder while empty.
    expect(h.terminal.output).toContain('add your feedback');
    // Tab must not fire an approval response.
    expect(h.respondToApproval).not.toHaveBeenCalled();
  });

  test('Tab opens an EMPTY notes input even when the global compose slot is stale', async () => {
    // A stray key (e.g. a 't' spammed at the y/t/n row) can leave a value in
    // the shared compose slot. Opening notes must clear it, not prefill it.
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setCommandInput('t');
    const h = mountApproval(store);
    await flush();
    h.terminal.output = '';
    h.terminal.sendInput(TAB);
    await flush();
    // Empty PromptInput shows its placeholder; the stale char must not prefill.
    expect(h.terminal.output).toContain('add your feedback');
    expect(store.getState().commandInputValue).toBe('');
  });

  test('submitting the feedback input stages the note without resolving', async () => {
    const h = mountApproval();
    await flush();
    h.terminal.sendInput(TAB);
    await flush();
    for (const ch of 'use sudo') h.terminal.sendInput(ch);
    await flush();
    h.terminal.output = '';
    h.terminal.sendInput(ENTER);
    await flush();
    // Staging returns to the picker — the approval is NOT resolved and no turn
    // is injected yet.
    expect(h.respondToApproval).not.toHaveBeenCalled();
    expect(h.onNotesSubmit).not.toHaveBeenCalled();
    // Back on the default page, the staged note is surfaced and [tab] flips to
    // "edit note".
    expect(h.terminal.output).toContain('note attached');
    expect(h.terminal.output).toContain('use sudo');
    expect(h.terminal.output).toContain('edit note');
  });

  test('staging a note then [y] sends the disposition AND flushes the note', async () => {
    const h = mountApproval();
    await flush();
    h.terminal.sendInput(TAB);
    await flush();
    for (const ch of 'use sudo') h.terminal.sendInput(ch);
    await flush();
    h.terminal.sendInput(ENTER); // stage, return to picker
    await flush();
    h.terminal.sendInput('y'); // pick disposition
    await flush();
    // The tool keeps the chosen disposition...
    expect(h.respondToApproval).toHaveBeenCalledWith(
      'allow_once',
      undefined,
      undefined
    );
    // ...and the staged note rides along as a follow-up turn (trimmed).
    expect(h.onNotesSubmit).toHaveBeenCalledTimes(1);
    expect(h.onNotesSubmit).toHaveBeenCalledWith('use sudo');
  });

  test('Esc from notes steps back to the default page (hotkey row returns)', async () => {
    const h = mountApproval();
    await flush();
    h.terminal.sendInput(TAB);
    await flush();
    expect(h.terminal.output).toContain('add your feedback');
    h.terminal.output = '';
    h.terminal.sendInput(ESC);
    await flush();
    // Back on the default page: the y/t/n hotkey row renders again and the
    // feedback input is gone. Esc must not have submitted notes or resolved.
    expect(h.terminal.output).toContain('add note');
    expect(h.terminal.output).not.toContain('add your feedback');
    expect(h.onNotesSubmit).not.toHaveBeenCalled();
    expect(h.respondToApproval).not.toHaveBeenCalled();
  });
});

// KAS ships shell trust scope in consentContext with EMPTY trustOptions. The
// fixtures above feed trustOptions manually, masking this real flow — these
// tests exercise it: a granular scope page must appear and whole-tool trust
// must send {kasWholeCapability:true} so resource:'*' persists (else KAS
// re-asks the same command).
function makeKasShellApproval() {
  return {
    toolId: 'execute_bash',
    toolCall: { toolCallId: 'call-1', title: 'execute_bash', rawInput: '' },
    permissionOptions: [
      { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
      { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
      { kind: ApprovalOptionId.AllowAlways, optionId: 'always-accept' },
    ],
    trustOptions: [],
    consentContext: { capability: 'shell', resource: 'git status' },
  };
}

function mountKasShellApproval(): Harness {
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  const terminal = new MockTerminal();
  const respondToApproval = vi.fn();
  const onNotesSubmit = vi.fn();
  const instance = render(
    <AppStoreContext.Provider value={store}>
      <ApprovalPrompt
        messages={[TOOL_MSG]}
        approval={makeKasShellApproval()}
        respondToApproval={respondToApproval}
        getStageInputColor={() => (t: string) => t}
        mainAgentName="main"
        onNotesSubmit={onNotesSubmit}
      />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  activeInstance = instance;
  return { terminal, respondToApproval, onNotesSubmit };
}

describe('ApprovalPrompt — KAS shell trust (empty trustOptions)', () => {
  test('[t] opens a granular scope page deriving rows from consentContext', async () => {
    const h = mountKasShellApproval();
    await flush();
    // Default page advertises trust scope (not "TRUST whole tool"), proving the
    // KAS scope page is detected from consentContext.
    expect(h.terminal.output).toContain('trust scope');
    h.terminal.output = '';
    h.terminal.sendInput('t');
    await flush();
    // The pattern row derived from the shell command is offered, and [t] did
    // NOT immediately resolve the approval (it opened the sub-page).
    expect(h.terminal.output).toContain('Trust "git *"');
    expect(h.terminal.output).toContain('[s] scope');
    expect(h.respondToApproval).not.toHaveBeenCalled();
  });

  test('selecting the entire-tool row sends {kasWholeCapability:true}', async () => {
    const h = mountKasShellApproval();
    await flush();
    h.terminal.sendInput('t'); // open scope page
    await flush();
    // Rows: [exact "git status", pattern "git *", entire tool]. Down twice to
    // land on the entire-tool row.
    h.terminal.sendInput('\x1b[B');
    h.terminal.sendInput('\x1b[B');
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();
    expect(h.respondToApproval).toHaveBeenCalledWith(
      'always-accept',
      undefined,
      {
        kasScope: 'session',
        kasWholeCapability: true,
      }
    );
  });

  test('selecting the pattern row sends kasResource (granular trust)', async () => {
    const h = mountKasShellApproval();
    await flush();
    h.terminal.sendInput('t'); // open scope page
    await flush();
    h.terminal.sendInput('\x1b[B'); // exact -> pattern
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();
    expect(h.respondToApproval).toHaveBeenCalledWith(
      'always-accept',
      undefined,
      {
        kasScope: 'session',
        kasResource: 'git *',
      }
    );
  });
});

// KAS write approvals also ship consentContext with EMPTY trustOptions, but a
// non-shell capability (fs_write). The scope page must still appear (deriving a
// single entire-tool row), and whole-tool trust must send
// {kasWholeCapability:true} so resource:'*' persists — else KAS re-asks every
// OTHER path after the first "trust whole tool".
function makeKasWriteApproval() {
  return {
    toolId: 'fs_write',
    toolCall: { toolCallId: 'call-1', title: 'fs_write', rawInput: '' },
    permissionOptions: [
      { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
      { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
      { kind: ApprovalOptionId.AllowAlways, optionId: 'always-accept' },
    ],
    trustOptions: [],
    consentContext: { capability: 'fs_write', resource: '/workspace/a.ts' },
  };
}

function mountKasWriteApproval(): Harness {
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  const terminal = new MockTerminal();
  const respondToApproval = vi.fn();
  const onNotesSubmit = vi.fn();
  const writeMsg: MessageType = {
    id: 'call-1',
    role: MessageRole.ToolUse,
    name: 'fs_write',
    content: JSON.stringify({ command: 'create', path: '/workspace/a.ts' }),
    isFinished: false,
  };
  const instance = render(
    <AppStoreContext.Provider value={store}>
      <ApprovalPrompt
        messages={[writeMsg]}
        approval={makeKasWriteApproval()}
        respondToApproval={respondToApproval}
        getStageInputColor={() => (t: string) => t}
        mainAgentName="main"
        onNotesSubmit={onNotesSubmit}
      />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  activeInstance = instance;
  return { terminal, respondToApproval, onNotesSubmit };
}

describe('ApprovalPrompt — KAS write trust (non-shell, empty trustOptions)', () => {
  test('[t] opens a granular scope page (not immediate whole-tool trust)', async () => {
    const h = mountKasWriteApproval();
    await flush();
    // Default page advertises trust scope, matching the full TUI — NOT the
    // bare "TRUST whole tool" the shell-only gate used to force for writes.
    expect(h.terminal.output).toContain('trust scope');
    expect(h.terminal.output).not.toContain('TRUST whole tool');
    h.terminal.output = '';
    h.terminal.sendInput('t');
    await flush();
    // [t] opened the sub-page (offering the exact path + entire tool) rather
    // than resolving the approval.
    expect(h.terminal.output).toContain('[s] scope');
    expect(h.terminal.output).toContain('/workspace/a.ts');
    expect(h.terminal.output).toContain('Trust entire tool');
    expect(h.respondToApproval).not.toHaveBeenCalled();
  });

  test('selecting the exact-path row sends kasResource (granular trust)', async () => {
    const h = mountKasWriteApproval();
    await flush();
    h.terminal.sendInput('t'); // open scope page
    await flush();
    // First row is the exact path; Enter trusts just that path.
    h.terminal.sendInput(ENTER);
    await flush();
    expect(h.respondToApproval).toHaveBeenCalledWith(
      'always-accept',
      undefined,
      {
        kasScope: 'session',
        kasResource: '/workspace/a.ts',
      }
    );
  });

  test('selecting the entire-tool row sends {kasWholeCapability:true}', async () => {
    const h = mountKasWriteApproval();
    await flush();
    h.terminal.sendInput('t'); // open scope page
    await flush();
    // Rows: [exact "/workspace/a.ts", entire tool]. Down once → entire-tool.
    h.terminal.sendInput('\x1b[B');
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();
    expect(h.respondToApproval).toHaveBeenCalledWith(
      'always-accept',
      undefined,
      {
        kasScope: 'session',
        kasWholeCapability: true,
      }
    );
  });
});

describe('handleNotesSubmit wiring (LiteLayout closure)', () => {
  // Faithful replica of the new LiteLayout.handleNotesSubmit. ApprovalPrompt
  // calls this only AFTER it has already sent the y/t/n disposition via
  // respondToApproval, so it must NOT cancel the approval (that would undo the
  // just-applied allow/trust — the original force-deny bug). It only injects
  // the staged text as a follow-up user turn. The non-empty guard lives in
  // ApprovalPrompt.respondWithNote, so this closure injects whatever it gets.
  function makeHandler() {
    const cancelApproval = vi.fn();
    const handleUserInput = vi.fn();
    const handleNotesSubmit = (value: string) => {
      handleUserInput(value);
    };
    return { cancelApproval, handleUserInput, handleNotesSubmit };
  }

  test('injects the note as a new turn without cancelling the approval', () => {
    const { cancelApproval, handleUserInput, handleNotesSubmit } =
      makeHandler();
    handleNotesSubmit('please use sudo');
    expect(handleUserInput).toHaveBeenCalledWith('please use sudo');
    // The disposition was already sent — cancelling here would force-deny it.
    expect(cancelApproval).not.toHaveBeenCalled();
  });
});
