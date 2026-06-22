import type { TestCase } from '../../../src/test-utils/TestCase';
import {
  AgentEventType,
  ApprovalOptionId,
  type PermissionOption,
  type TrustOption,
} from '../../../src/types/agent-events';

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
