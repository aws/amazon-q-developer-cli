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

const notice = (id: string): MessageType => ({
  id,
  role: MessageRole.System,
  content: 'Autonomous mode on',
  success: true,
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

  it('leaves an inter-turn command notice out of the completed previous turn', () => {
    // Regression: a system notice emitted BETWEEN turns (e.g. the /autonomous
    // toggle line, added while no prompt is in flight) must not be adopted
    // into the previous turn's card. Folding it there re-renders it above
    // content that was emitted before it (observed with the autonomous toggle
    // notice appearing above an earlier turn-error row).
    const messages = [
      user('turn-1'),
      model('answer-1'),
      notice('autonomous-on'),
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

  it('keeps a mid-turn system row inside its turn body', () => {
    // Counterpart guard: a system row that lands while the turn is still
    // producing body (more turn messages follow it) stays inline where it
    // happened.
    const messages = [
      user('turn-1'),
      model('answer-1'),
      notice('mid-turn-notice'),
      model('answer-2'),
    ];
    const turns = includeInterleavedSystemRows(
      groupMessagesIntoTurns(
        messages.filter((message) => message.role !== MessageRole.System)
      ),
      messages
    );

    expect(turns[0]?.aiMessages.map((message) => message.id)).toEqual([
      'answer-1',
      'mid-turn-notice',
      'answer-2',
    ]);
  });

  it('keeps a turn-owned system row in its turn even when trailing', () => {
    // turnOwned rows (e.g. the turn-failure line stamped while isProcessing)
    // belong to the turn that produced them even as its last message.
    const messages = [
      user('turn-1'),
      model('answer-1'),
      {
        ...notice('turn-error'),
        success: false,
        turnOwned: true,
      } as MessageType,
    ];
    const turns = includeInterleavedSystemRows(
      groupMessagesIntoTurns(
        messages.filter((message) => message.role !== MessageRole.System)
      ),
      messages
    );

    expect(turns[0]?.aiMessages.map((message) => message.id)).toEqual([
      'answer-1',
      'turn-error',
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
