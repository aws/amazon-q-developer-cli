import { expect } from 'bun:test';
import type { TestCase } from '../../../src/test-utils/TestCase';
import {
  AgentEventType,
  ApprovalOptionId,
  type PermissionOption,
  type TrustOption,
} from '../../../src/types/agent-events';

const ALLOW_ONCE: PermissionOption = {
  kind: ApprovalOptionId.AllowOnce,
  name: 'Allow Once',
  optionId: 'allow_once',
};
const REJECT_ONCE: PermissionOption = {
  kind: ApprovalOptionId.RejectOnce,
  name: 'Reject Once',
  optionId: 'reject_once',
};

/** Allow Once / Allow Always / Reject Once — the common 3-button option set. */
const ALLOW_ALWAYS_REJECT_OPTIONS: PermissionOption[] = [
  ALLOW_ONCE,
  {
    kind: ApprovalOptionId.AllowAlways,
    name: 'Allow Always',
    optionId: 'allow_always',
  },
  REJECT_ONCE,
];

/** Allow Once / Reject Once — the 2-button set (subagent/inner approvals). */
export const ALLOW_REJECT_OPTIONS: PermissionOption[] = [
  ALLOW_ONCE,
  REJECT_ONCE,
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
 * Inject (optionally) a ToolCall followed by an ApprovalRequest. rawInput and
 * the option-set are regression-sensitive, so callers pass them explicitly.
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

/** Assert the approval prompt is painted (`painted`) or deferred (`!painted`) via the "needs approval" marker. */
export function expectApprovalPainted(tc: TestCase, painted: boolean): void {
  const snap = tc.getSnapshot().join('\n');
  if (painted) expect(snap).toContain('needs approval');
  else expect(snap).not.toContain('needs approval');
}
