import { describe, it, expect, mock } from 'bun:test';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import { handleRewind } from '../rewind';
import { MessageRole } from '../../../stores/app-store';
import type { KasCommand } from '../../../kas-commands';

const REWIND_CMD: KasCommand = {
  name: 'rewind' as any,
  description: 'Rewind to earlier turn',
  meta: { inputType: 'panel' },
};

function makeMessages(turns: string[]) {
  const msgs: Array<{
    id: string;
    role: string;
    content: string;
    kasMessageId?: string;
  }> = [];
  turns.forEach((label, i) => {
    msgs.push({
      id: `local-${i}`,
      role: MessageRole.User,
      content: label,
      kasMessageId: `kas-${i}`,
    });
    msgs.push({
      id: `resp-${i}`,
      role: MessageRole.Model,
      content: `Answer to ${label}`,
    });
  });
  return msgs;
}

describe('handleRewind fork', () => {
  it('passes the selected turn kasMessageId (not array index) to session/fork', async () => {
    const messages = makeMessages([
      'What is aws?',
      'What is cloudwatch?',
      'What is ec2?',
      'What is ecs?',
    ]);
    const executeCommand = mock(() =>
      Promise.resolve({
        success: true,
        message: '',
        data: { sessionId: 'sess_forked', switchSession: true },
      })
    );
    const loadSession = mock((_id: string, _cb: any) =>
      Promise.resolve({ currentModel: 'auto', currentAgent: null })
    );
    const ctx = createMockCommandContext({
      messages: messages as any,
      kiro: { executeCommand, loadSession } as any,
    });

    // Picker builds turns reversed: [ecs(3), ec2(2), cloudwatch(1), aws(0)]
    // User selects cloudwatch -> row.id = "1" (logIndex)
    await handleRewind(REWIND_CMD, '1', ctx as any);

    // Should have called executeCommand with cloudwatch's kasMessageId
    expect(executeCommand).toHaveBeenCalledTimes(1);
    const call = (executeCommand as any).mock.calls[0][0];
    expect(call.command).toBe('rewind');
    expect(call.args.messageId).toBe('kas-1'); // cloudwatch's kasMessageId, NOT kas-2
  });

  it('opens picker when no args', async () => {
    const messages = makeMessages(['aws', 'cloudwatch']);
    const ctx = createMockCommandContext({ messages: messages as any });

    await handleRewind(REWIND_CMD, '', ctx as any);

    expect(ctx._spies.setShowRewindExplorer).toHaveBeenCalledTimes(1);
    const [visible, turns] = (ctx._spies.setShowRewindExplorer as any).mock
      .calls[0];
    expect(visible).toBe(true);
    expect(turns).toHaveLength(2);
    // Reversed: newest first
    expect(turns[0].label).toBe('cloudwatch');
    expect(turns[1].label).toBe('aws');
  });

  it('shows alert when turn index out of range', async () => {
    const messages = makeMessages(['aws']);
    const ctx = createMockCommandContext({ messages: messages as any });

    await handleRewind(REWIND_CMD, '99', ctx as any);

    expect(ctx._spies.showAlert).toHaveBeenCalledTimes(1);
  });

  it('uses logIndex to find turn (not array position)', async () => {
    // 3 turns: aws(0), cloudwatch(1), ec2(2)
    // Reversed list: [ec2(logIndex:2), cloudwatch(logIndex:1), aws(logIndex:0)]
    // Selecting logIndex=2 should find ec2, not the item at array[2] (which is aws)
    const messages = makeMessages([
      'What is aws?',
      'What is cloudwatch?',
      'What is ec2?',
    ]);
    const executeCommand = mock(() =>
      Promise.resolve({
        success: true,
        message: '',
        data: { sessionId: 'sess_f', switchSession: true },
      })
    );
    const loadSession = mock((_id: string, _cb: any) =>
      Promise.resolve({ currentModel: 'auto', currentAgent: null })
    );
    const ctx = createMockCommandContext({
      messages: messages as any,
      kiro: { executeCommand, loadSession } as any,
    });

    await handleRewind(REWIND_CMD, '2', ctx as any);

    const call = (executeCommand as any).mock.calls[0][0];
    expect(call.args.messageId).toBe('kas-2'); // ec2's ID, not aws's
  });
});
