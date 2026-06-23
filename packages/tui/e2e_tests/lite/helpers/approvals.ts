import { expect } from 'bun:test';
import type { TestCase } from '../../../src/test-utils/TestCase';
import type { E2ETestCase } from '../../E2ETestCase';
import {
  AgentEventType,
  ApprovalOptionId,
  type PermissionOption,
  type TrustOption,
} from '../../../src/types/agent-events';

/**
 * Push a `write` ToolUseEvent (which triggers a create-file approval) plus the
 * terminating null, over the e2e agent IPC. Mirrors the inline blocks the
 * approval-swap e2e suites use to drive a pending approval to the prompt.
 */
export async function pushWriteApprovalEvent(
  tc: E2ETestCase,
  opts: { toolUseId: string; path: string; content: string }
): Promise<void> {
  await tc.pushSendMessageResponse([
    {
      kind: 'event',
      data: {
        kind: 'ToolUseEvent',
        data: {
          tool_use_id: opts.toolUseId,
          name: 'write',
          input: JSON.stringify({
            command: 'create',
            path: opts.path,
            content: opts.content,
          }),
          stop: true,
        },
      },
    },
  ]);
  await tc.pushSendMessageResponse(null);
}

/** Allow Once / Allow Always / Reject Once — the common 3-button option set. */
export const ALLOW_ALWAYS_REJECT_OPTIONS: PermissionOption[] = [
  {
    kind: ApprovalOptionId.AllowOnce,
    name: 'Allow Once',
    optionId: 'allow_once',
  },
  {
    kind: ApprovalOptionId.AllowAlways,
    name: 'Allow Always',
    optionId: 'allow_always',
  },
  {
    kind: ApprovalOptionId.RejectOnce,
    name: 'Reject Once',
    optionId: 'reject_once',
  },
];

/** Allow Once / Reject Once — the 2-button option set (subagent/inner approvals). */
export const ALLOW_REJECT_OPTIONS: PermissionOption[] = [
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
];

export interface InjectApprovalOpts {
  toolCallId: string;
  toolName: string;
  sessionId?: string;
  /** Defaults to ALLOW_ALWAYS_REJECT_OPTIONS. */
  options?: PermissionOption[];
  trustOptions?: TrustOption[];
  /** rawInput on the approval's toolCall (and the preceding ToolCall args). */
  rawInput?: Record<string, unknown>;
  /** kind of the preceding ToolCall (default 'shell'). */
  toolKind?: string;
  /**
   * Inject a matching ToolCall before the ApprovalRequest (default true). The
   * auto-expand path seeds its ToolCall separately and passes false.
   */
  withPrecedingToolCall?: boolean;
  /** Sleep after the ApprovalRequest (default 0). */
  settleMs?: number;
}

/**
 * Inject (optionally) a ToolCall followed by an ApprovalRequest. Mirrors the
 * shape both the approval-flow and auto-expand integ suites depend on; the
 * rawInput/option-set are regression-sensitive so callers pass them explicitly
 * where they diverge.
 */
export async function injectApproval(
  tc: TestCase,
  opts: InjectApprovalOpts
): Promise<void> {
  const rawInput = opts.rawInput ?? { command: `echo ${opts.toolCallId}` };
  const sessionField = opts.sessionId ? { sessionId: opts.sessionId } : {};

  if (opts.withPrecedingToolCall !== false) {
    await tc.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: opts.toolCallId,
      name: opts.toolName,
      kind: (opts.toolKind ?? 'shell') as any,
      args: rawInput,
      ...sessionField,
    } as any);
  }

  await tc.mockSessionUpdate({
    type: AgentEventType.ApprovalRequest,
    value: {
      ...sessionField,
      toolCall: {
        toolCallId: opts.toolCallId,
        title: opts.toolName,
        rawInput,
      },
      permissionOptions: opts.options ?? ALLOW_ALWAYS_REJECT_OPTIONS,
      ...(opts.trustOptions ? { trustOptions: opts.trustOptions } : {}),
      resolve: (() => {
        /* noop — tests drive the store-side clear by pressing y/n */
      }) as any,
    },
  } as any);

  if (opts.settleMs) await tc.sleepMs(opts.settleMs);
}

/** Assert the approval prompt is painted ("needs approval" visible). */
export function expectApprovalVisible(tc: TestCase): void {
  expect(tc.getSnapshot().join('\n')).toContain('needs approval');
}

/** Assert the approval is deferred: in store but not yet painted. */
export function expectApprovalDeferred(tc: TestCase): void {
  expect(tc.getSnapshot().join('\n')).not.toContain('needs approval');
}
