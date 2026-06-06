import { describe, test, expect } from 'vitest';
import { shouldCancelApprovalForKilledStage } from '../subagent-kill.js';
import {
  MessageRole,
  type MessageType,
} from '../../../../stores/app-store.js';

function tool(
  id: string,
  opts: { agentName?: string; name?: string } = {}
): MessageType {
  return {
    id,
    role: MessageRole.ToolUse,
    name: opts.name ?? 'shell',
    content: '',
    isFinished: false,
    ...(opts.agentName ? { agentName: opts.agentName } : {}),
  };
}

function user(id: string): MessageType {
  return { id, role: MessageRole.User, content: 'q' };
}

function approval(
  toolCallId: string | null
): { toolCall: { toolCallId: string | null } } {
  return { toolCall: { toolCallId } };
}

describe('shouldCancelApprovalForKilledStage', () => {
  test('false when no approval pending', () => {
    expect(shouldCancelApprovalForKilledStage(null, [], 'stage-a')).toBe(false);
  });

  test('false when approval has no toolCallId', () => {
    expect(
      shouldCancelApprovalForKilledStage(approval(null), [], 'stage-a')
    ).toBe(false);
  });

  test('false when the matching tool isn\'t in messages (race)', () => {
    // Backend may surface the approval RPC slightly before the ToolUse
    // message lands in `messages`. Killing the stage in that window
    // shouldn't drop a different stage's approval — bail safely.
    const msgs = [user('u1'), tool('other-tool', { agentName: 'stage-b' })];
    expect(
      shouldCancelApprovalForKilledStage(approval('missing'), msgs, 'stage-a')
    ).toBe(false);
  });

  test('false when approval is for the MAIN agent (no agentName)', () => {
    // Main-agent tool calls have no agentName attached. Killing a
    // subagent stage must not clear the parent's own approval prompt.
    const msgs = [tool('parent-write', {})];
    expect(
      shouldCancelApprovalForKilledStage(
        approval('parent-write'),
        msgs,
        'stage-a'
      )
    ).toBe(false);
  });

  test('false when approval is for a DIFFERENT subagent stage', () => {
    // Two stages running in parallel, both with pending tools. Killing
    // stage A shouldn't dismiss stage B's approval.
    const msgs = [
      tool('a-write', { agentName: 'stage-a' }),
      tool('b-write', { agentName: 'stage-b' }),
    ];
    expect(
      shouldCancelApprovalForKilledStage(approval('b-write'), msgs, 'stage-a')
    ).toBe(false);
  });

  test('true when approval belongs to the killed stage', () => {
    const msgs = [tool('a-write', { agentName: 'stage-a' })];
    expect(
      shouldCancelApprovalForKilledStage(approval('a-write'), msgs, 'stage-a')
    ).toBe(true);
  });
});
