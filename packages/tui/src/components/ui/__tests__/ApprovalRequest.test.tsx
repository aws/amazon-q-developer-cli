import { afterEach, describe, expect, test, vi } from 'vitest';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  ToolUseStatus,
  type MessageType,
} from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import {
  ApprovalOptionId,
  type ApprovalRequestInfo,
} from '../../../types/agent-events.js';
import { ApprovalRequest } from '../ApprovalRequest.js';
import { ToolUseMessage } from '../ToolUseMessage.js';
import { VerbosityOverrideContext } from '../../../hooks/useVerbose.js';
import {
  DEFAULT_DISPLAY,
  type VerboseDisplayConfig,
} from '../../../lite/verbose.js';

const DOWN = '\x1b[B';
const ENTER = '\r';
const originalLiteRollout = process.env.KIRO_LITE_ROLLOUT_ENABLED;

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
  if (originalLiteRollout === undefined) {
    delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
  } else {
    process.env.KIRO_LITE_ROLLOUT_ENABLED = originalLiteRollout;
  }
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

function mountApprovalRequest(
  approval: ApprovalRequestInfo,
  messages: MessageType[] = [TOOL_MSG],
  agentEngine: 'v2' | 'kas' = 'kas'
) {
  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine,
  });
  const terminal = new MockTerminal();
  const respondToApproval = vi.fn();
  store.setState({
    approvalMode: 'dropdown',
    messages,
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

function makeToolApproval(toolMsg: MessageType): ApprovalRequestInfo {
  if (toolMsg.role !== MessageRole.ToolUse) {
    throw new Error('Expected a tool-use message');
  }

  return {
    toolId: toolMsg.name,
    toolCall: {
      toolCallId: toolMsg.id,
      title: toolMsg.originalTitle ?? toolMsg.name,
      rawInput: JSON.parse(toolMsg.content),
    },
    permissionOptions: [
      {
        kind: ApprovalOptionId.AllowOnce,
        name: 'Allow Once',
        optionId: 'allow_once',
      },
      {
        kind: ApprovalOptionId.RejectOnce,
        name: 'Reject Once',
        optionId: 'reject_once',
      },
    ],
    trustOptions: [],
    resolve: vi.fn(),
  };
}

function mountTuiApprovalSurface(
  toolMsg: MessageType,
  displayOverride: Partial<VerboseDisplayConfig> = {}
) {
  if (toolMsg.role !== MessageRole.ToolUse) {
    throw new Error('Expected a tool-use message');
  }

  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'v2',
  });
  const terminal = new MockTerminal();
  const approval = makeToolApproval(toolMsg);
  store.setState({
    approvalMode: 'dropdown',
    messages: [toolMsg],
    pendingApproval: approval,
    respondToApproval: vi.fn(),
  });

  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <VerbosityOverrideContext.Provider
        value={{
          display: { ...DEFAULT_DISPLAY, ...displayOverride },
          filters: [],
        }}
      >
        <ToolUseMessage
          id={toolMsg.id}
          name={toolMsg.name}
          kind={toolMsg.kind}
          origin={toolMsg.origin}
          content={toolMsg.content}
          diff={toolMsg.diff}
          isFinished={false}
          status={ToolUseStatus.Pending}
        />
        <ApprovalRequest onDrillInSubmit={vi.fn()} />
      </VerbosityOverrideContext.Provider>
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );

  return terminal;
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

describe('ApprovalRequest tool payload rendering', () => {
  for (const agentEngine of ['v2', 'kas'] as const) {
    test(`renders preserved raw input through the tool painter when ${agentEngine} permission arrives before ToolUse`, async () => {
      const approval = makeKasShellApproval();
      approval.toolCall.rawInput = {
        command: 'echo permission-first-payload',
        cwd: '/workspace',
      };
      const { terminal } = mountApprovalRequest(approval, [], agentEngine);
      await flush();
      const output = stripAnsi(terminal.output);

      expect(output).toContain('execute_bash requires approval');
      expect(output).toContain('Shell echo permission-first-payload');
      expect(output).toContain('cwd=/workspace');
    });
  }

  test('renders a permission-first built-in write as a diff', async () => {
    const approval = makeKasShellApproval();
    approval.toolId = 'fs_write';
    approval.toolCall = {
      toolCallId: 'permission-first-write',
      title: 'Creating report.ts',
      name: 'fs_write',
      kind: 'edit',
      origin: 'builtin',
      rawInput: {
        command: 'str_replace',
        path: '/workspace/report.ts',
        oldStr: 'export const ready = false;',
        newStr: 'export const ready = true;',
      },
    };
    const { terminal } = mountApprovalRequest(approval, [], 'kas');
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('Write');
    expect(output).toContain('/workspace/report.ts');
    expect(output).toContain('export const ready = false;');
    expect(output).toContain('export const ready = true;');
    expect(output).not.toContain('oldStr=');
  });

  test('keeps a permission-first MCP write collision generic', async () => {
    const approval = makeKasShellApproval();
    approval.toolId = 'fs_write';
    approval.toolCall = {
      toolCallId: 'permission-first-mcp',
      title: '@server/fs_write',
      name: 'fs_write',
      kind: 'edit',
      origin: 'mcp',
      rawInput: {
        command: 'str_replace',
        path: '/workspace/report.ts',
        old_str: 'before',
        new_str: 'after',
      },
    };
    const { terminal } = mountApprovalRequest(approval, [], 'kas');
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('fs_write requires approval');
    expect(output).toContain('command=str_replace');
    expect(output).toContain('old_str=before');
    expect(output).not.toContain('Write');
    expect(output).not.toContain('added 1 line');
  });

  test('renders an eligible edit-kind tool as a write diff', async () => {
    const terminal = mountTuiApprovalSurface(
      {
        id: 'call-write',
        role: MessageRole.ToolUse,
        name: 'Patch Workspace',
        kind: 'edit',
        content: JSON.stringify({
          command: 'str_replace',
          path: '/workspace/file.ts',
          oldStr: 'const value = 1;',
          newStr: 'const value = 2;',
        }),
        diff: {
          path: '/workspace/file.ts',
          oldText: 'const value = 1;',
          newText: 'const value = 2;',
        },
        isFinished: false,
      },
      {
        showWriteDiffs: false,
        outputMaxLines: 1,
        outputMaxChars: 1,
      }
    );
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('Write');
    expect(output).toContain('/workspace/file.ts');
    expect(output).toContain('added 1 line');
    expect(output).toContain('removed 1 line');
    expect(output).toContain('const value = 1;');
    expect(output).toContain('const value = 2;');
  });

  test('renders a parent subagent with the dedicated orchestration painter', async () => {
    process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
    const terminal = mountTuiApprovalSurface(
      {
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
      },
      {
        toolArgsMode: 'off',
        subagent: {
          pipeline: false,
          prompts: false,
          roles: false,
          deps: false,
          responses: false,
        },
      }
    );
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('Orchestrating');
    expect(output).toContain('(1 agent)');
    expect(output).toContain('Inspect the approval components');
    expect(output).toContain('orchestrate_subagent requires approval');
    expect(output).not.toContain('stages=');
  });

  test('renders ordinary tool arguments with the generic painter', async () => {
    const terminal = mountTuiApprovalSurface(
      {
        id: 'call-generic',
        role: MessageRole.ToolUse,
        name: 'custom_inspector',
        content: JSON.stringify({
          query: 'approval routing',
          limit: 2,
        }),
        isFinished: false,
      },
      {
        toolArgsMode: 'off',
        argsMaxLines: 1,
        argsMaxChars: 1,
      }
    );
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('custom_inspector');
    expect(output).toContain('query=approval routing');
    expect(output).toContain('limit=2');
    expect(output).not.toContain('added 1 line');
  });

  test('keeps an MCP edit-kind name collision generic and cannot render its diff', async () => {
    const terminal = mountTuiApprovalSurface({
      id: 'call-mcp',
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
      diff: {
        path: '/workspace/file.ts',
        oldText: 'before',
        newText: 'after',
      },
      isFinished: false,
    });
    await flush();
    const output = stripAnsi(terminal.output);

    expect(output).toContain('fs_write');
    expect(output).toContain('command=str_replace');
    expect(output).toContain('old_str=before');
    expect(output).toContain('new_str=after');
    expect(output).not.toContain('Write');
    expect(output).not.toContain('added 1 line');
    expect(output).not.toContain('removed 1 line');
  });
});
