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
 * Drive a few render cycles + drain microtasks so React effects flush AND
 * twinki has registered its stdin handler. The first commit after render()
 * wires terminal.start(onInput) asynchronously, so a too-short wait drops the
 * first keystroke (observed as flaky first-test failures). 20ms + two
 * microtask drains is comfortably past the commit on CI-class hardware.
 */
async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 20));
  await Promise.resolve();
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

function mountApproval(): Harness {
  const store = createAppStore({ kiro: new Kiro() });
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

const READ_TRUST_OPTION = {
  label: 'Exact file',
  display: 'subdir/file.txt',
  setting_key: 'allowedPaths',
  patterns: ['subdir/file.txt'],
};

function makeKasReadTrustApproval() {
  return {
    toolId: 'fs_read',
    toolCall: {
      toolCallId: 'tooluse_read_1',
      title: 'fs_read',
    },
    permissionOptions: [
      { kind: ApprovalOptionId.AllowOnce, optionId: 'accept' },
      { kind: ApprovalOptionId.RejectOnce, optionId: 'reject' },
      { kind: ApprovalOptionId.AllowAlways, optionId: 'always-accept' },
    ],
    trustOptions: [READ_TRUST_OPTION],
  };
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
