import { afterEach, describe, expect, it, mock } from 'bun:test';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { render, type Instance, type Terminal } from 'twinki';
import { Kiro } from '../../../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
} from '../../../../stores/app-store.js';
import {
  createWorkflowStore,
  workflowStore,
} from '../../../../stores/workflow-store.js';
import {
  ApprovalOptionId,
  type ApprovalRequestInfo,
} from '../../../../types/agent-events.js';
import type { WorkflowNodeSessionTarget } from '../../../../types/workflow.js';
import type {
  WorkflowCancelResponse,
  WorkflowInspectResponse,
  WorkflowPauseResponse,
  WorkflowRetryResponse,
  WorkflowRunSummary,
} from '../../../../types/workflow-history.js';
import { WorkflowHistoryPanel } from '../WorkflowHistoryPanel.js';
import { WorkflowMonitorScreen } from '../WorkflowMonitorScreen.js';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  output = '';

  constructor(
    private readonly columnCount = 120,
    private readonly rowCount = 40
  ) {}

  get columns() {
    return this.columnCount;
  }

  get rows() {
    return this.rowCount;
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
  workflowStore.getState().reset();
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 50));
  await Promise.resolve();
}

function makeApproval(): ApprovalRequestInfo {
  return {
    sessionId: 'child-1',
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
        kind: ApprovalOptionId.RejectOnce,
        name: 'Reject Once',
        optionId: 'reject_once',
      },
    ],
    resolve: () => {},
  };
}

function openMessageableWorkflow(name: string): void {
  workflowStore.getState().openHistoricalWorkflow({
    workflowId: 'workflow-1',
    parentSessionId: 'parent-1',
    name,
    status: 'running',
    nodes: [
      {
        id: 'step-1',
        type: 'step',
        status: 'running',
        label: 'coder',
        parentId: null,
        depth: 0,
        sessionId: 'child-1',
        agentName: 'coder',
      },
    ],
    stepSessions: [
      {
        nodeId: 'step-1',
        nodePath: ['step-1'],
        sessionId: 'child-1',
        status: 'running',
        agentName: 'coder',
      },
    ],
    startedAt: Date.now(),
    completedAt: null,
  });
}

function openMultiNodeMessageableWorkflow(): void {
  workflowStore.getState().openHistoricalWorkflow({
    workflowId: 'workflow-1',
    parentSessionId: 'parent-1',
    name: 'Multi-node drafts',
    status: 'running',
    nodes: [
      {
        id: 'step-1',
        type: 'step',
        status: 'running',
        label: 'coder',
        parentId: null,
        depth: 0,
        sessionId: 'child-1',
        agentName: 'coder',
      },
      {
        id: 'step-2',
        type: 'step',
        status: 'running',
        label: 'reviewer',
        parentId: null,
        depth: 0,
        sessionId: 'child-2',
        agentName: 'reviewer',
      },
    ],
    stepSessions: [
      {
        nodeId: 'step-1',
        nodePath: ['step-1'],
        sessionId: 'child-1',
        status: 'running',
        agentName: 'coder',
      },
      {
        nodeId: 'step-2',
        nodePath: ['step-2'],
        sessionId: 'child-2',
        status: 'running',
        agentName: 'reviewer',
      },
    ],
    startedAt: Date.now(),
    completedAt: null,
  });
}

describe('WorkflowMonitorScreen', () => {
  it('renders a retained workflow in the alternate screen', async () => {
    workflowStore.getState().openHistoricalWorkflow({
      workflowId: 'workflow-1',
      parentSessionId: 'parent-1',
      name: 'Reconnect validation',
      status: 'running',
      nodes: [
        {
          id: 'step-1',
          type: 'step',
          status: 'running',
          label: 'coder',
          parentId: null,
          depth: 0,
          sessionId: 'child-1',
          agentName: 'coder',
        },
      ],
      stepSessions: [],
      startedAt: Date.now(),
      completedAt: null,
    });

    const appStore = createAppStore({
      kiro: new Kiro(),
      agentEngine: 'kas',
    });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    const output = stripAnsi(terminal.output);
    expect(terminal.output).toContain('\x1b[?1049h');
    expect(output).toContain('WORKFLOW(S)');
    expect(output).toContain('Reconnect validation');
    expect(output).toContain('coder');
  });

  it('sends one retry when r repeats while the request is pending', async () => {
    const store = createWorkflowStore();
    store.getState().openHistoricalWorkflow({
      workflowId: 'workflow-1',
      parentSessionId: 'parent-1',
      name: 'Retry validation',
      status: 'failed',
      nodes: [
        {
          id: 'step-1',
          type: 'step',
          status: 'failed',
          label: 'coder',
          parentId: null,
          depth: 0,
          sessionId: 'child-1',
          agentName: 'coder',
        },
      ],
      stepSessions: [],
      startedAt: Date.now(),
      completedAt: Date.now(),
    });
    let resolveRetry!: (response: WorkflowRetryResponse) => void;
    const retryWorkflow = mock(
      (_workflowId: string) =>
        new Promise<WorkflowRetryResponse>((resolve) => {
          resolveRetry = resolve;
        })
    );
    const kiro = new Kiro();
    kiro.retryWorkflow = retryWorkflow;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen store={store} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false, mouse: true }
    );
    await flush();

    expect(stripAnsi(terminal.output)).toContain('r retry');
    terminal.sendInput('r');
    terminal.sendInput('r');
    await flush();

    expect(retryWorkflow).toHaveBeenCalledTimes(1);
    // Chat alone can't restart a terminal run, so `r` names the selected node.
    expect(retryWorkflow).toHaveBeenCalledWith('workflow-1', 'step-1');

    resolveRetry({
      workflowId: 'workflow-1',
      status: 'running',
      retriedNodeIds: ['step-1'],
    });
    await flush();
  });

  it('retries the whole run for a failed step inside a loop', async () => {
    // A bare nodeId resolved first-DFS can't name one iteration. Retrying more
    // than asked is recoverable; retrying a different iteration is not.
    const store = createWorkflowStore();
    store.getState().openHistoricalWorkflow({
      workflowId: 'workflow-1',
      parentSessionId: 'parent-1',
      name: 'Loop retry validation',
      status: 'failed',
      nodes: [
        {
          id: 'review',
          type: 'step',
          status: 'failed',
          label: 'reviewer',
          parentId: 'loop',
          depth: 1,
          iteration: 2,
          sessionId: 'child-1',
          agentName: 'reviewer',
        },
      ],
      stepSessions: [],
      startedAt: Date.now(),
      completedAt: Date.now(),
    });
    const retryWorkflow = mock(async () => ({
      workflowId: 'workflow-1',
      status: 'running' as const,
      retriedNodeIds: ['review'],
    }));
    const kiro = new Kiro();
    kiro.retryWorkflow = retryWorkflow;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen store={store} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false, mouse: true }
    );
    await flush();

    terminal.sendInput('r');
    await flush();

    expect(retryWorkflow).toHaveBeenCalledWith('workflow-1', undefined);
  });

  it('asks nothing of the user on a run they stopped themselves', async () => {
    // Already explained by the header banner — calling the same mechanical reason
    // a question says two contradictory things about one stop.
    const store = createWorkflowStore();
    store.getState().openHistoricalWorkflow({
      workflowId: 'workflow-1',
      parentSessionId: 'parent-1',
      name: 'Stop attribution validation',
      status: 'paused',
      pauseReason: "Workflow paused before node 'review'.",
      stopInitiator: 'user',
      nodes: [
        {
          id: 'review',
          type: 'step',
          status: 'paused',
          label: 'reviewer',
          parentId: null,
          depth: 0,
          sessionId: 'child-1',
          agentName: 'reviewer',
          pauseReason: "Workflow paused before node 'review'.",
        },
      ],
      stepSessions: [],
      startedAt: Date.now(),
      completedAt: null,
    });
    const appStore = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen store={store} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false, mouse: true }
    );
    await flush();

    const output = stripAnsi(terminal.output);
    expect(output).toContain('Stopped by you.');
    expect(output).not.toContain('Waiting on you');
  });

  it('surfaces the question a step is parked on', async () => {
    const store = createWorkflowStore();
    store.getState().openHistoricalWorkflow({
      workflowId: 'workflow-1',
      parentSessionId: 'parent-1',
      name: 'Question validation',
      status: 'paused',
      nodes: [
        {
          id: 'review',
          type: 'step',
          status: 'paused',
          label: 'reviewer',
          parentId: null,
          depth: 0,
          sessionId: 'child-1',
          agentName: 'reviewer',
          pauseReason: 'Ship the release candidate?',
        },
      ],
      stepSessions: [],
      startedAt: Date.now(),
      completedAt: null,
    });
    const appStore = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen store={store} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false, mouse: true }
    );
    await flush();

    const output = stripAnsi(terminal.output);
    // The mock terminal is narrow enough to wrap the banner, so match the
    // prefix rather than the whole reason.
    expect(output).toContain('Waiting on you: Ship the release');
  });

  it('keeps successful workflow messages out of the main alert bar', async () => {
    openMessageableWorkflow('Isolation validation');

    const messages: Array<{
      target: WorkflowNodeSessionTarget;
      content: string;
    }> = [];
    const kiro = new Kiro();
    kiro.messageWorkflowNode = async (target, content) => {
      messages.push({ target, content });
    };
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();
    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    terminal.sendInput('s');
    await flush();
    expect(stripAnsi(terminal.output)).toContain('Steer');
    terminal.sendInput('Keep this inside the workflow');
    await flush();
    terminal.sendInput('\r');
    await flush();

    expect(messages).toHaveLength(1);
    expect(messages[0]?.content).toBe('Keep this inside the workflow');
    expect(appStore.getState().transientAlert).toBeNull();
    expect(workflowStore.getState().inputActive).toBe(false);
  });

  it('retains separate message drafts while navigating workflow nodes', async () => {
    openMultiNodeMessageableWorkflow();

    const messages: Array<{
      target: WorkflowNodeSessionTarget;
      content: string;
    }> = [];
    const kiro = new Kiro();
    kiro.messageWorkflowNode = async (target, content) => {
      messages.push({ target, content });
    };
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();
    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    terminal.sendInput('s');
    await flush();
    terminal.sendInput('Draft for coder');
    terminal.sendInput('\x1b[B');
    await flush();
    terminal.sendInput('Draft for reviewer');
    terminal.sendInput('\x1b[A');
    await flush();
    terminal.sendInput('\r');
    await flush();

    expect(messages[0]).toMatchObject({
      target: { workflowId: 'workflow-1', sessionId: 'child-1' },
      content: 'Draft for coder',
    });

    terminal.sendInput('\x1b[B');
    await flush();
    terminal.sendInput('s');
    await flush();
    terminal.sendInput('\r');
    await flush();

    expect(messages[1]).toMatchObject({
      target: { workflowId: 'workflow-1', sessionId: 'child-2' },
      content: 'Draft for reviewer',
    });
  });

  // #19: up/down navigate between nodes while the steer/respond composer is
  // open; they must move the selection *without* tearing the composer down.
  it('keeps the steer composer open while navigating with up/down', async () => {
    openMultiNodeMessageableWorkflow();

    const kiro = new Kiro();
    kiro.messageWorkflowNode = async () => {};
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();
    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    terminal.sendInput('s');
    await flush();
    expect(workflowStore.getState().inputActive).toBe(true);

    terminal.sendInput('\x1b[B'); // down
    await flush();
    expect(workflowStore.getState().inputActive).toBe(true);

    terminal.sendInput('\x1b[A'); // up
    await flush();
    expect(workflowStore.getState().inputActive).toBe(true);
  });

  // Verify Up/Down navigation works alongside EditorInput without losing typed text
  it('navigates between nodes with up/down while EditorInput is active', async () => {
    openMultiNodeMessageableWorkflow();

    const messages: Array<{
      target: WorkflowNodeSessionTarget;
      content: string;
    }> = [];
    const kiro = new Kiro();
    kiro.messageWorkflowNode = async (target, content) => {
      messages.push({ target, content });
    };
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();
    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    // Open composer on first node, type text, then navigate down
    terminal.sendInput('s');
    await flush();
    terminal.sendInput('message for first');
    await flush();
    terminal.sendInput('\x1b[B'); // Down — navigate to second node
    await flush();

    // Should still be in input mode (composer stays open)
    expect(workflowStore.getState().inputActive).toBe(true);

    // Type on second node and submit
    terminal.sendInput('message for second');
    await flush();
    terminal.sendInput('\r');
    await flush();

    // Should have sent to the second node
    expect(messages).toHaveLength(1);
    expect(messages[0]!.content).toBe('message for second');
    expect(messages[0]!.target.sessionId).toBe('child-2');
  });

  it('restores the submitted workflow message when sending fails', async () => {
    openMessageableWorkflow('Failure recovery');

    let rejectMessage!: (error: Error) => void;
    const sendMessage = mock(
      (_target: WorkflowNodeSessionTarget, _content: string) =>
        new Promise<void>((_resolve, reject) => {
          rejectMessage = reject;
        })
    );
    const kiro = new Kiro();
    kiro.messageWorkflowNode = sendMessage;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();
    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    terminal.sendInput('s');
    await flush();
    terminal.sendInput('Recover this message');
    await flush();
    terminal.sendInput('\r');
    await flush();
    expect(workflowStore.getState().inputActive).toBe(false);

    rejectMessage(new Error('Workflow session is unavailable'));
    await flush();

    expect(workflowStore.getState().inputActive).toBe(true);
    expect(appStore.getState().transientAlert?.message).toBe(
      'Workflow session is unavailable'
    );

    terminal.sendInput('\r');
    await flush();
    expect(sendMessage).toHaveBeenCalledTimes(2);
    expect(sendMessage.mock.calls[1]?.[1]).toBe('Recover this message');
  });

  it('does not restore an older failed send over a newer submission', async () => {
    openMessageableWorkflow('Failure ordering');

    const pending: Array<{
      resolve: () => void;
      reject: (error: Error) => void;
    }> = [];
    const sendMessage = mock(
      (_target: WorkflowNodeSessionTarget, _content: string) =>
        new Promise<void>((resolve, reject) => {
          pending.push({ resolve, reject });
        })
    );
    const kiro = new Kiro();
    kiro.messageWorkflowNode = sendMessage;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();
    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    terminal.sendInput('s');
    await flush();
    terminal.sendInput('First message');
    await flush();
    terminal.sendInput('\r');
    await flush();
    terminal.sendInput('s');
    await flush();
    terminal.sendInput('Second message');
    await flush();
    terminal.sendInput('\r');
    await flush();
    expect(pending).toHaveLength(2);
    expect(workflowStore.getState().inputActive).toBe(false);

    pending[0]?.reject(new Error('First send failed'));
    await flush();
    expect(workflowStore.getState().inputActive).toBe(false);

    pending[1]?.reject(new Error('Second send failed'));
    await flush();
    expect(workflowStore.getState().inputActive).toBe(true);

    terminal.sendInput('\r');
    await flush();
    expect(sendMessage).toHaveBeenCalledTimes(3);
    expect(sendMessage.mock.calls[2]?.[1]).toBe('Second message');
    pending[2]?.resolve();
  });

  it('uses the current node state when restoring a failed send', async () => {
    openMessageableWorkflow('Completed while sending');

    let rejectMessage!: (error: Error) => void;
    const sendMessage = mock(
      (_target: WorkflowNodeSessionTarget, _content: string) =>
        new Promise<void>((_resolve, reject) => {
          rejectMessage = reject;
        })
    );
    const kiro = new Kiro();
    kiro.messageWorkflowNode = sendMessage;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();
    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    terminal.sendInput('s');
    await flush();
    terminal.sendInput('Retry after completion');
    await flush();
    terminal.sendInput('\r');
    await flush();

    const currentWorkflow = workflowStore
      .getState()
      .workflows.get('workflow-1');
    workflowStore.setState({
      workflows: new Map(workflowStore.getState().workflows).set('workflow-1', {
        ...currentWorkflow!,
        status: 'completed',
        nodes: currentWorkflow!.nodes.map((node) => ({
          ...node,
          status: 'completed',
        })),
        completedAt: Date.now(),
      }),
    });
    await flush();

    rejectMessage(new Error('Workflow session changed'));
    await flush();

    expect(workflowStore.getState().inputActive).toBe(true);
    expect(stripAnsi(terminal.output)).toContain('Message');
    terminal.sendInput('\r');
    await flush();
    expect(sendMessage.mock.calls[1]?.[1]).toBe('Retry after completion');
  });

  it('lets Escape leave the monitor while an approval is displayed', async () => {
    workflowStore.getState().openHistoricalWorkflow({
      workflowId: 'workflow-1',
      parentSessionId: 'parent-1',
      name: 'Approval validation',
      status: 'running',
      nodes: [
        {
          id: 'step-1',
          type: 'step',
          status: 'running',
          label: 'coder',
          parentId: null,
          depth: 0,
          sessionId: 'child-1',
          agentName: 'coder',
        },
      ],
      stepSessions: [],
      startedAt: Date.now(),
      completedAt: null,
    });

    const appStore = createAppStore({
      kiro: new Kiro(),
      agentEngine: 'kas',
    });
    appStore.setState({
      mode: 'workflow-monitor',
      approvalQueue: [makeApproval()],
    });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowMonitorScreen />
      </AppStoreContext.Provider>,
      {
        terminal,
        exitOnCtrlC: false,
        patchConsole: false,
        mouse: true,
      }
    );
    await flush();

    terminal.sendInput('\x1b');
    await flush();

    expect(appStore.getState().mode).toBe('inline');
  });
});

describe('WorkflowHistoryPanel', () => {
  it('retries the selected failed workflow with r', async () => {
    workflowStore.getState().openWorkflowHistory([
      {
        workflowId: 'workflow-1',
        name: 'Failed release',
        status: 'failed',
        createdAt: '2026-07-20T10:00:00.000Z',
        updatedAt: '2026-07-20T10:01:00.000Z',
        parentSessionId: 'parent-1',
      },
    ]);
    let resolveRetry!: (response: WorkflowRetryResponse) => void;
    const retryWorkflow = mock(
      (_workflowId: string) =>
        new Promise<WorkflowRetryResponse>((resolve) => {
          resolveRetry = resolve;
        })
    );
    const kiro = new Kiro();
    kiro.retryWorkflow = retryWorkflow;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowHistoryPanel onClose={() => {}} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false }
    );
    await flush();

    expect(stripAnsi(terminal.output)).toContain('r retry');
    terminal.sendInput('r');
    terminal.sendInput('r');
    await flush();
    expect(retryWorkflow).toHaveBeenCalledTimes(1);
    expect(stripAnsi(terminal.output)).toContain('retrying...');

    resolveRetry({
      workflowId: 'workflow-1',
      status: 'running',
      retriedNodeIds: ['step-1'],
    });
    await flush();
    expect(workflowStore.getState().history.runs[0]?.status).toBe('running');
  });

  it('pauses and resumes the selected workflow with p and r', async () => {
    const run: WorkflowRunSummary = {
      workflowId: 'workflow-1',
      name: 'Release validation',
      status: 'running',
      createdAt: '2026-07-20T10:00:00.000Z',
      updatedAt: '2026-07-20T10:01:00.000Z',
      parentSessionId: 'parent-1',
    };
    workflowStore.getState().openWorkflowHistory([run]);
    const kiro = new Kiro();
    let resolvePause!: (response: WorkflowPauseResponse) => void;
    const pauseWorkflow = mock(
      (_workflowId: string) =>
        new Promise<WorkflowPauseResponse>((resolve) => {
          resolvePause = resolve;
        })
    );
    const resumeWorkflow = mock(async (workflowId: string) => ({
      workflowId,
      status: 'running' as const,
    }));
    kiro.pauseWorkflow = pauseWorkflow;
    kiro.resumeWorkflow = resumeWorkflow;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowHistoryPanel onClose={() => {}} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false }
    );
    await flush();
    expect(stripAnsi(terminal.output)).toContain('p pause');
    expect(stripAnsi(terminal.output)).toContain('x cancel');

    terminal.sendInput('p');
    terminal.sendInput('p');
    await flush();
    expect(pauseWorkflow).toHaveBeenCalledWith('workflow-1');
    expect(pauseWorkflow).toHaveBeenCalledTimes(1);
    expect(workflowStore.getState().history.runs[0]?.status).toBe('running');
    expect(stripAnsi(terminal.output)).toContain('pausing...');

    resolvePause({ paused: true });
    await flush();
    expect(workflowStore.getState().history.runs[0]?.status).toBe('paused');
    expect(stripAnsi(terminal.output)).toContain('r resume');
    expect(stripAnsi(terminal.output)).toContain('x cancel');

    terminal.sendInput('r');
    await flush();
    expect(resumeWorkflow).toHaveBeenCalledWith('workflow-1');
    expect(workflowStore.getState().history.runs[0]?.status).toBe('running');
  });

  it('confirms cancellation and shows the pending status in narrow panels', async () => {
    workflowStore.getState().openWorkflowHistory([
      {
        workflowId: 'workflow-1',
        name: 'A workflow name that must yield space to its status',
        status: 'running',
        createdAt: '2026-07-20T10:00:00.000Z',
        updatedAt: '2026-07-20T10:01:00.000Z',
        parentSessionId: 'parent-1',
      },
    ]);
    let resolveCancel!: (response: WorkflowCancelResponse) => void;
    const cancelWorkflow = mock(
      (_workflowId: string, _targetStatus?: 'aborted' | 'completed') =>
        new Promise<WorkflowCancelResponse>((resolve) => {
          resolveCancel = resolve;
        })
    );
    const kiro = new Kiro();
    kiro.cancelWorkflow = cancelWorkflow;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal(48);

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowHistoryPanel onClose={() => {}} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false }
    );
    await flush();

    expect(stripAnsi(terminal.output)).toContain('p pause');
    expect(stripAnsi(terminal.output)).toContain('x cancel');
    terminal.sendInput('x');
    await flush();

    expect(cancelWorkflow).not.toHaveBeenCalled();
    expect(stripAnsi(terminal.output)).toContain('x confirm cancel');

    terminal.output = '';
    terminal.sendInput('\x1b');
    await flush();
    expect(cancelWorkflow).not.toHaveBeenCalled();
    expect(stripAnsi(terminal.output)).not.toContain('x confirm cancel');

    terminal.sendInput('x');
    await flush();
    terminal.sendInput('x');
    await flush();
    expect(cancelWorkflow).toHaveBeenCalledTimes(1);
    expect(cancelWorkflow).toHaveBeenCalledWith('workflow-1', 'aborted');
    expect(stripAnsi(terminal.output)).toContain('cancelling...');
    expect(workflowStore.getState().history.runs[0]?.status).toBe('running');

    resolveCancel({ ok: true, previousStatus: 'running' });
    await flush();
    expect(workflowStore.getState().history.runs[0]?.status).toBe('aborted');
  });

  it('keeps the authoritative terminal status when cancellation loses a race', async () => {
    workflowStore.getState().openWorkflowHistory([
      {
        workflowId: 'workflow-1',
        name: 'Already finished',
        status: 'running',
        createdAt: '2026-07-20T10:00:00.000Z',
        updatedAt: '2026-07-20T10:01:00.000Z',
        parentSessionId: 'parent-1',
      },
    ]);
    const kiro = new Kiro();
    kiro.cancelWorkflow = mock(async () => ({
      ok: true,
      previousStatus: 'completed' as const,
    }));
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowHistoryPanel onClose={() => {}} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false }
    );
    await flush();

    terminal.sendInput('x');
    terminal.sendInput('x');
    await flush();

    expect(workflowStore.getState().history.runs[0]?.status).toBe('completed');
  });

  it('opens a run only once when Enter repeats', async () => {
    workflowStore.getState().openWorkflowHistory([
      {
        workflowId: 'workflow-1',
        name: 'Release',
        status: 'completed',
        createdAt: '2026-07-21T00:00:00.000Z',
        updatedAt: '2026-07-21T00:01:00.000Z',
        parentSessionId: 'parent-1',
      },
    ]);

    let resolveInspect!: (value: WorkflowInspectResponse) => void;
    const inspectWorkflow = mock(
      () =>
        new Promise<WorkflowInspectResponse>((resolve) => {
          resolveInspect = resolve;
        })
    );
    const kiro = new Kiro();
    kiro.inspectWorkflow = inspectWorkflow as typeof kiro.inspectWorkflow;
    const appStore = createAppStore({ kiro, agentEngine: 'kas' });
    const onClose = mock(() => {});
    const terminal = new MockTerminal();

    activeInstance = render(
      <AppStoreContext.Provider value={appStore}>
        <WorkflowHistoryPanel onClose={onClose} />
      </AppStoreContext.Provider>,
      { terminal, exitOnCtrlC: false, patchConsole: false }
    );
    await flush();

    terminal.sendInput('\r');
    terminal.sendInput('\r');
    await flush();

    expect(inspectWorkflow).toHaveBeenCalledTimes(1);

    resolveInspect({
      workflowId: 'workflow-1',
      state: {
        workflowId: 'workflow-1',
        workflowName: 'Release',
        status: 'completed',
        parentSessionId: 'parent-1',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        root: {
          nodeId: 'root',
          type: 'step',
          status: 'completed',
        },
      },
    });
    await flush();

    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
