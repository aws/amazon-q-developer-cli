import { afterEach, describe, expect, test, vi } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  type MessageType,
} from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import {
  ApprovalOptionId,
  type ApprovalRequestInfo,
} from '../../../types/agent-events.js';
import { ApprovalRequest } from '../ApprovalRequest.js';

const DOWN = '\x1b[B';
const ENTER = '\r';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
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
    this.onInput = onInput;
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
    this.onInput?.(data);
  }
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

const TOOL_MSG: MessageType = {
  id: 'call-1',
  role: MessageRole.ToolUse,
  name: 'execute_bash',
  content: JSON.stringify({ command: 'echo hello' }),
  isFinished: false,
};

function makeKasShellApproval(): ApprovalRequestInfo {
  return {
    toolId: 'execute_bash',
    toolCall: {
      toolCallId: 'call-1',
      title: 'execute_bash',
      rawInput: { command: 'echo hello' },
    },
    permissionOptions: [
      {
        kind: ApprovalOptionId.AllowOnce,
        name: 'Allow Once',
        optionId: 'allow_once',
      },
      {
        kind: ApprovalOptionId.AllowAlways,
        name: 'Always',
        optionId: 'always-accept',
      },
      {
        kind: ApprovalOptionId.RejectOnce,
        name: 'Reject Once',
        optionId: 'reject_once',
      },
    ],
    trustOptions: [
      {
        label: 'Exact command',
        display: 'echo hello',
        setting_key: 'allowedCommands',
        patterns: ['echo hello'],
      },
    ],
    consentContext: {
      capability: 'shell',
      resource: 'echo hello',
    },
    resolve: vi.fn(),
  };
}

function mountApprovalRequest(approval: ApprovalRequestInfo) {
  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'kas',
  });
  const terminal = new MockTerminal();
  const respondToApproval = vi.fn();
  store.setState({
    approvalMode: 'dropdown',
    messages: [TOOL_MSG],
    pendingApproval: approval,
    respondToApproval,
  });

  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <ApprovalRequest onDrillInSubmit={vi.fn()} />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );

  return { terminal, respondToApproval };
}

describe('ApprovalRequest KAS shell trust', () => {
  test('legacy trust-options entire-tool selection uses the KAS allow-always optionId', async () => {
    const approval = makeKasShellApproval();
    const h = mountApprovalRequest(approval);
    await flush();

    h.terminal.sendInput(DOWN);
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();

    expect(h.terminal.output).toContain('trust options');

    h.terminal.sendInput(DOWN);
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();

    expect(h.respondToApproval).toHaveBeenCalledWith(
      'always-accept',
      undefined,
      {
        kasWholeCapability: true,
      }
    );
  });
});
