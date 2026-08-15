import { describe, expect, it } from 'bun:test';
import type { Key } from '../../../../hooks/useKeypress.js';
import type { WorkflowRunView } from '../../../../types/workflow-monitor.js';
import { UNICODE_GLYPHS } from '../../../../utils/glyphs.js';
import { classifyInputKey } from '../classify-input-key.js';
import {
  buildMonitorFooterHints,
  type MonitorFooterHint,
} from '../monitor-footer-hints.js';
import {
  monitorPaneDimensions,
  resizeMonitorRatio,
} from '../monitor-layout.js';
import { runStatusLabel } from '../run-status-style.js';
import { fitWorkflowNodeMetadata } from '../workflow-node-format.js';
import { workflowControlShortcut } from '../workflow-control-shortcut.js';
import {
  adjacentWorkflowId,
  buildWorkflowTabs,
  rollupWorkflowCounts,
  workflowDigitToIndex,
} from '../workflow-tabs.js';
import { classifyWorkflowStopKey } from '../workflow-stop-confirmation.js';

function hasHint(
  hints: MonitorFooterHint[],
  key: string,
  label: string
): boolean {
  return hints.some((hint) => hint.key === key && hint.label === label);
}

const key = (overrides: Partial<Key> = {}): Key => ({
  upArrow: false,
  downArrow: false,
  leftArrow: false,
  rightArrow: false,
  pageUp: false,
  pageDown: false,
  home: false,
  end: false,
  return: false,
  escape: false,
  ctrl: false,
  shift: false,
  meta: false,
  tab: false,
  backspace: false,
  delete: false,
  ...overrides,
});

const workflow = (
  workflowId: string,
  status: WorkflowRunView['status']
): WorkflowRunView => ({
  workflowId,
  parentSessionId: 'parent',
  name: workflowId,
  status,
  nodes: [
    {
      id: `${workflowId}-step`,
      type: 'step',
      status: status === 'completed' ? 'completed' : 'running',
      label: 'coder',
      parentId: null,
      depth: 0,
    },
  ],
  stepSessions: [],
  startedAt: 1,
  completedAt: status === 'completed' ? 2 : null,
});

describe('workflow monitor view model', () => {
  it('builds indexed workflow tabs with step progress', () => {
    const tabs = buildWorkflowTabs(
      [workflow('alpha', 'running'), workflow('beta', 'completed')],
      UNICODE_GLYPHS,
      (token) => token
    );

    expect(tabs.map((tab) => tab.title)).toEqual(['alpha 0/1', 'beta 1/1']);
    expect(tabs.map((tab) => tab.status)).toEqual(['running', 'completed']);
  });

  it('rolls up all terminal and live statuses', () => {
    expect(
      rollupWorkflowCounts([
        workflow('a', 'running'),
        workflow('b', 'paused'),
        workflow('c', 'completed'),
        workflow('d', 'aborted'),
      ])
    ).toEqual({ running: 1, paused: 1, completed: 1, failed: 1 });
  });

  it('cycles and jumps retained workflow tabs', () => {
    expect(adjacentWorkflowId(['a', 'b', 'c'], 'c', 1)).toBe('a');
    expect(adjacentWorkflowId(['a', 'b', 'c'], 'a', -1)).toBe('c');
    expect(workflowDigitToIndex('2', 3)).toBe(1);
    expect(workflowDigitToIndex('4', 3)).toBeNull();
  });

  it('computes stable side-by-side and stacked dimensions', () => {
    expect(monitorPaneDimensions('side-by-side', 101, 20, 0.4)).toEqual({
      dagWidth: 40,
      dagHeight: 20,
      outputWidth: 60,
      outputHeight: 20,
    });
    expect(monitorPaneDimensions('stacked', 80, 21, 0.5)).toEqual({
      dagWidth: 80,
      dagHeight: 10,
      outputWidth: 80,
      outputHeight: 10,
    });
    expect(resizeMonitorRatio(0.2, -0.1)).toBe(0.2);
    expect(resizeMonitorRatio(0.8, 0.1)).toBe(0.8);
  });

  it('shows the richest model and effort detail that fits the DAG row', () => {
    const node = {
      modelId: 'claude-fable-5',
      effortLevel: 'high',
    };

    expect(fitWorkflowNodeMetadata(node, 23, UNICODE_GLYPHS.smallDot)).toBe(
      '  claude-fable-5 · high'
    );
    expect(fitWorkflowNodeMetadata(node, 22, UNICODE_GLYPHS.smallDot)).toBe(
      '  claude-fable-5'
    );
    expect(fitWorkflowNodeMetadata(node, 15, UNICODE_GLYPHS.smallDot)).toBe(
      '  high'
    );
    expect(fitWorkflowNodeMetadata(node, 5, UNICODE_GLYPHS.smallDot)).toBe('');
  });

  it('contains text input including paste and delete', () => {
    expect(classifyInputKey('', key({ escape: true }))).toBe('cancel');
    expect(classifyInputKey('', key({ return: true }))).toBe('submit');
    expect(classifyInputKey('', key({ backspace: true }))).toBe('delete');
    expect(classifyInputKey('hello\nignored', key({ paste: true }))).toEqual({
      append: 'hello',
    });
  });

  it('makes stop confirmation modal', () => {
    expect(classifyWorkflowStopKey('x', key({ ctrl: true }), false, true)).toBe(
      'arm'
    );
    expect(classifyWorkflowStopKey('a', key(), true, true)).toBe('block');
    expect(classifyWorkflowStopKey('', key({ escape: true }), true, true)).toBe(
      'dismiss'
    );
    expect(classifyWorkflowStopKey('x', key({ ctrl: true }), true, true)).toBe(
      'confirm'
    );
  });

  it('keeps completed child messaging discoverable', () => {
    const hints = buildMonitorFooterHints({
      selectedNode: {
        id: 'done',
        type: 'step',
        status: 'completed',
        label: 'coder',
        parentId: null,
        depth: 0,
        sessionId: 'done-session',
      },
      status: 'completed',
      monitorLayout: 'side-by-side',
      mouseModeEnabled: false,
      stopConfirmationArmed: false,
      inputOpen: false,
      glyphs: UNICODE_GLYPHS,
    });

    expect(hasHint(hints, 's', 'message')).toBe(true);
    expect(hasHint(hints, 'ctrl+x', 'stop')).toBe(false);
  });

  it('names the two motions a failed step needs', () => {
    // Chatting fixes nothing on its own, so the footer advertises both motions.
    const hints = buildMonitorFooterHints({
      selectedNode: {
        id: 'broken',
        type: 'step',
        status: 'failed',
        label: 'coder',
        parentId: null,
        depth: 0,
        sessionId: 'broken-session',
      },
      status: 'failed',
      monitorLayout: 'side-by-side',
      mouseModeEnabled: false,
      stopConfirmationArmed: false,
      inputOpen: false,
      glyphs: UNICODE_GLYPHS,
    });

    expect(hasHint(hints, 's', 'message')).toBe(true);
    expect(hasHint(hints, 'r', 'retry step')).toBe(true);
  });

  it('does not promise a step-scoped retry inside a loop', () => {
    // A loop-body step retries the whole run, since the request cannot name one
    // iteration, so the footer must not promise a step-scoped retry.
    const hints = buildMonitorFooterHints({
      selectedNode: {
        id: 'broken',
        type: 'step',
        status: 'failed',
        label: 'coder',
        parentId: 'loop',
        depth: 1,
        iteration: 2,
        sessionId: 'broken-session',
      },
      status: 'failed',
      monitorLayout: 'side-by-side',
      mouseModeEnabled: false,
      stopConfirmationArmed: false,
      inputOpen: false,
      glyphs: UNICODE_GLYPHS,
    });

    expect(hasHint(hints, 'r', 'retry')).toBe(true);
    expect(hasHint(hints, 'r', 'retry step')).toBe(false);
  });

  it('offers a reply at a paused step regardless of completion signal', () => {
    const hints = buildMonitorFooterHints({
      selectedNode: {
        id: 'parked',
        type: 'step',
        status: 'paused',
        label: 'reviewer',
        parentId: null,
        depth: 0,
        sessionId: 'parked-session',
      },
      status: 'paused',
      monitorLayout: 'side-by-side',
      mouseModeEnabled: false,
      stopConfirmationArmed: false,
      inputOpen: false,
      glyphs: UNICODE_GLYPHS,
    });

    expect(hasHint(hints, 's', 'respond')).toBe(true);
    expect(hasHint(hints, 'r', 'resume')).toBe(true);
  });

  it('does not offer a reply at a parked container', () => {
    // A message is addressed to a session and a container owns none, so the
    // composer refuses it — advertising `s respond` there is a dead key.
    const hints = buildMonitorFooterHints({
      selectedNode: {
        id: 'branches',
        type: 'parallel',
        status: 'paused',
        label: 'branches',
        parentId: null,
        depth: 0,
      },
      status: 'paused',
      monitorLayout: 'side-by-side',
      mouseModeEnabled: false,
      stopConfirmationArmed: false,
      inputOpen: false,
      glyphs: UNICODE_GLYPHS,
    });

    expect(hasHint(hints, 's', 'respond')).toBe(false);
    expect(hasHint(hints, 'r', 'resume')).toBe(true);
  });

  it('does not offer a fix message at a failed container', () => {
    const hints = buildMonitorFooterHints({
      selectedNode: {
        id: 'loop',
        type: 'repeat',
        status: 'failed',
        label: 'loop',
        parentId: null,
        depth: 0,
      },
      status: 'failed',
      monitorLayout: 'side-by-side',
      mouseModeEnabled: false,
      stopConfirmationArmed: false,
      inputOpen: false,
      glyphs: UNICODE_GLYPHS,
    });

    expect(hasHint(hints, 's', 'message')).toBe(false);
    // The whole run, since a container is not a step KAS can rerun on its own.
    expect(hasHint(hints, 'r', 'retry')).toBe(true);
    expect(hasHint(hints, 'r', 'retry step')).toBe(false);
  });

  it('does not offer a message at a step with no session yet', () => {
    const hints = buildMonitorFooterHints({
      selectedNode: {
        id: 'queued',
        type: 'step',
        status: 'paused',
        label: 'reviewer',
        parentId: null,
        depth: 0,
      },
      status: 'paused',
      monitorLayout: 'side-by-side',
      mouseModeEnabled: false,
      stopConfirmationArmed: false,
      inputOpen: false,
      glyphs: UNICODE_GLYPHS,
    });

    expect(hasHint(hints, 's', 'respond')).toBe(false);
  });

  it('shows optimistic graceful pause state', () => {
    expect(runStatusLabel('running', true)).toBe('pausing...');
    expect(runStatusLabel('paused', true)).toBe('paused');
  });

  it('advertises lowercase pause and resume controls', () => {
    const base = {
      selectedNode: null,
      monitorLayout: 'side-by-side' as const,
      mouseModeEnabled: false,
      stopConfirmationArmed: false,
      inputOpen: false,
      glyphs: UNICODE_GLYPHS,
    };

    expect(
      hasHint(
        buildMonitorFooterHints({ ...base, status: 'running' }),
        'p',
        'pause'
      )
    ).toBe(true);
    expect(
      hasHint(
        buildMonitorFooterHints({ ...base, status: 'paused' }),
        'r',
        'resume'
      )
    ).toBe(true);
    expect(
      hasHint(
        buildMonitorFooterHints({ ...base, status: 'failed' }),
        'r',
        'retry'
      )
    ).toBe(true);
    expect(workflowControlShortcut('p', key(), 'running')).toBe('pause');
    expect(workflowControlShortcut('r', key(), 'paused')).toBe('resume');
    expect(workflowControlShortcut('r', key(), 'failed')).toBe('retry');
    expect(workflowControlShortcut('r', key(), 'aborted')).toBe('retry');
    expect(workflowControlShortcut('r', key(), 'completed')).toBeNull();
    expect(
      workflowControlShortcut('p', key({ ctrl: true }), 'running')
    ).toBeNull();
  });
});
