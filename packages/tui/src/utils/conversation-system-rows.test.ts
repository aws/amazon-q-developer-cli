import { describe, expect, it } from 'bun:test';
import { MessageRole, type MessageType } from '../stores/app-store.js';
import { groupMessagesIntoTurns } from './group-turns.js';
import { includeInterleavedSystemRows } from './conversation-system-rows.js';

const user = (id: string): MessageType => ({
  id,
  role: MessageRole.User,
  content: id,
});

const model = (id: string): MessageType => ({
  id,
  role: MessageRole.Model,
  content: id,
});

const completion = (id: string, workflowTurnId?: string): MessageType => ({
  id,
  role: MessageRole.System,
  content: 'Workflow "release" completed',
  success: true,
  kind: 'workflow-completion',
  workflowId: 'wf-1',
  workflowName: 'release',
  workflowStatus: 'completed',
  ...(workflowTurnId ? { workflowTurnId } : {}),
});

describe('includeInterleavedSystemRows', () => {
  it('leaves a late workflow completion outside already-emitted turns', () => {
    const messages = [
      user('turn-1'),
      model('answer-1'),
      user('turn-2'),
      model('answer-2'),
      completion('workflow-complete', 'turn-1'),
    ];
    const turns = includeInterleavedSystemRows(
      groupMessagesIntoTurns(
        messages.filter((message) => message.role !== MessageRole.System)
      ),
      messages
    );

    expect(turns[0]?.aiMessages.map((message) => message.id)).toEqual([
      'answer-1',
    ]);
    expect(turns[1]?.aiMessages.map((message) => message.id)).toEqual([
      'answer-2',
    ]);
  });

  it('leaves an unowned workflow completion out of unrelated turns', () => {
    const messages = [
      user('turn-1'),
      model('answer-1'),
      completion('workflow-complete'),
    ];
    const turns = includeInterleavedSystemRows(
      groupMessagesIntoTurns(
        messages.filter((message) => message.role !== MessageRole.System)
      ),
      messages
    );

    expect(turns[0]?.aiMessages.map((message) => message.id)).toEqual([
      'answer-1',
    ]);
  });
});
