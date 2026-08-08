import { describe, it, expect } from 'vitest';
import { selectPanelAnchor } from '../subagent-panel-anchor.js';
import { MessageRole, type MessageType } from '../../stores/app-store.js';

/**
 * Which card mounts SubagentToolPanel, and what group it scopes to.
 *
 * The panel lists every live sub-agent session, so it must mount exactly once. The
 * selection also has to mirror the conversation render loop's filters: a message the
 * loop drops can still win here, and then the panel mounts nowhere at all — strictly
 * worse than mounting it under every card.
 */

function wrapper(
  overrides: Partial<Extract<MessageType, { role: MessageRole.ToolUse }>> = {}
): MessageType {
  return {
    id: 'w',
    role: MessageRole.ToolUse,
    name: 'Sub-agent: context-gatherer',
    kind: 'other',
    content: '{}',
    isFinished: false,
    ...overrides,
  } as MessageType;
}

const model = (id: string): MessageType =>
  ({ id, role: MessageRole.Model, content: 'text' }) as MessageType;

describe('selectPanelAnchor', () => {
  it('returns no anchor when nothing is in flight', () => {
    expect(selectPanelAnchor([model('m1')]).index).toBe(-1);
  });

  it('anchors the last in-flight sub-agent card', () => {
    const messages = [
      wrapper({ id: 'a' }),
      model('m1'),
      wrapper({ id: 'b' }),
      model('m2'),
    ];

    expect(selectPanelAnchor(messages).index).toBe(2);
  });

  it('ignores finished cards', () => {
    const messages = [
      wrapper({ id: 'a' }),
      wrapper({ id: 'b', isFinished: true }),
    ];

    expect(selectPanelAnchor(messages).index).toBe(0);
  });

  it('skips cards the render loop drops, so the panel still mounts somewhere', () => {
    // The regression this pins: a nested wrapper carries isSubagentTool, the render
    // loop returns null for it, and anchoring there mounted the panel nowhere at all.
    const messages = [
      wrapper({ id: 'visible' }),
      wrapper({ id: 'nested', isSubagentTool: true }),
    ];

    expect(selectPanelAnchor(messages).index).toBe(0);
  });

  it('ignores non-subagent tool calls', () => {
    const readFile = {
      id: 'r',
      role: MessageRole.ToolUse,
      name: 'fs_write',
      kind: 'edit',
      content: '{}',
      isFinished: false,
    } as MessageType;

    expect(selectPanelAnchor([wrapper({ id: 'a' }), readFile]).index).toBe(0);
  });

  it('accepts wire-name spawn cards too', () => {
    const messages = [
      wrapper({ id: 'a', name: 'invoke_sub_agent', kind: undefined }),
    ];

    expect(selectPanelAnchor(messages).index).toBe(0);
  });

  describe('group scoping', () => {
    it('scopes to the shared group when every in-flight card has it', () => {
      const messages = [
        wrapper({ id: 'a', pipelineGroupId: 'g1' }),
        wrapper({ id: 'b', pipelineGroupId: 'g1' }),
      ];

      expect(selectPanelAnchor(messages).groupId).toBe('g1');
    });

    it('drops the scope when two groups are live so neither is hidden', () => {
      const messages = [
        wrapper({ id: 'a', pipelineGroupId: 'g1' }),
        wrapper({ id: 'b', pipelineGroupId: 'g2' }),
      ];

      // undefined makes selectSubagentToolSessions skip its group filter and return
      // the union of live sessions.
      expect(selectPanelAnchor(messages).groupId).toBeUndefined();
    });

    it('leaves the scope undefined when cards carry no group', () => {
      expect(selectPanelAnchor([wrapper({ id: 'a' })]).groupId).toBeUndefined();
    });

    it('ignores the group of a card it skipped', () => {
      const messages = [
        wrapper({ id: 'a', pipelineGroupId: 'g1' }),
        wrapper({ id: 'skipped', pipelineGroupId: 'g2', isSubagentTool: true }),
      ];

      expect(selectPanelAnchor(messages).groupId).toBe('g1');
    });
  });
});
