import { describe, it, expect, mock } from 'bun:test';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({})),
}));

import { createMessageStreamHandler } from '../message-stream-handler';
import { AgentEventType, ContentType } from '../../types/agent-events';
import { MessageRole, type MessageType } from '../app-store';

function setup(initialMessages: MessageType[] = []) {
  let messages = [...initialMessages];
  const getMessages = () => messages;
  const setMessages = (updater: (msgs: MessageType[]) => MessageType[]) => {
    messages = updater(messages);
  };
  const getAgentName = mock(() => 'test-agent');
  const handler = createMessageStreamHandler(
    getMessages,
    setMessages,
    getAgentName
  );
  return { handler, getMessages, setMessages, getAgentName };
}

describe('createMessageStreamHandler', () => {
  describe('Content events', () => {
    it('buffers text and flushes after timer', async () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: { type: ContentType.Text, text: 'hello' },
      });

      // Before flush, messages might not be updated yet
      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      expect(msgs).toHaveLength(1);
      expect(msgs[0]!.role).toBe(MessageRole.Model);
      expect(msgs[0]!.content).toBe('hello');
    });

    it('accumulates multiple content events', async () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: { type: ContentType.Text, text: 'hello ' },
      });
      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: { type: ContentType.Text, text: 'world' },
      });

      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      expect(msgs).toHaveLength(1);
      expect(msgs[0]!.content).toBe('hello world');
    });

    it('updates existing model message instead of appending', async () => {
      const initial: MessageType[] = [
        { id: 'm1', role: MessageRole.Model, content: 'old text' },
      ];
      const { handler, getMessages } = setup(initial);

      handler({
        type: AgentEventType.Content,
        id: 'm1',
        content: { type: ContentType.Text, text: 'new text' },
      });

      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      expect(msgs).toHaveLength(1);
      expect(msgs[0]!.content).toBe('new text');
    });

    it('uses getAgentName callback for attribution', async () => {
      const { handler, getMessages, getAgentName } = setup();

      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: { type: ContentType.Text, text: 'hello' },
      });

      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      expect(msgs[0]!.role).toBe(MessageRole.Model);
      expect((msgs[0] as any).agentName).toBe('test-agent');
      expect(getAgentName).toHaveBeenCalled();
    });
  });

  describe('ToolCall events', () => {
    it('flushes pending content and adds tool message', async () => {
      const { handler, getMessages } = setup();

      // Buffer some content first
      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: { type: ContentType.Text, text: 'some text' },
      });

      // ToolCall should flush content immediately and add tool
      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'fs_write',
        args: { path: '/test.txt' },
      });

      // Content should be flushed immediately by the ToolCall
      const msgs = getMessages();
      expect(msgs.length).toBeGreaterThanOrEqual(1);
      // The last message should be the tool
      const toolMsg = msgs.find(
        (m) => m.role === MessageRole.ToolUse && m.id === 'tool1'
      );
      expect(toolMsg).toBeDefined();
    });

    it('updates existing tool if same ID with new args', () => {
      const initial: MessageType[] = [
        {
          id: 'tool1',
          role: MessageRole.ToolUse,
          name: 'fs_write',
          content: '{}',
        } as MessageType,
      ];
      const { handler, getMessages } = setup(initial);

      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'fs_write',
        args: { path: '/updated.txt' },
      });

      const msgs = getMessages();
      const toolMsg = msgs.find(
        (m) => m.role === MessageRole.ToolUse && m.id === 'tool1'
      );
      expect(toolMsg).toBeDefined();
      expect(toolMsg!.content).toContain('/updated.txt');
    });

    it('adds new tool message when ID not found', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCall,
        id: 'new-tool',
        name: 'grep',
        args: { query: 'test' },
      });

      const msgs = getMessages();
      expect(msgs).toHaveLength(1);
      expect(msgs[0]!.role).toBe(MessageRole.ToolUse);
      expect(msgs[0]!.id).toBe('new-tool');
    });
  });

  describe('ToolCallUpdate events', () => {
    it('appends live output lines and drops trailing empty string', () => {
      const initial: MessageType[] = [
        {
          id: 'tool1',
          role: MessageRole.ToolUse,
          name: 'shell',
          content: '{}',
        } as MessageType,
      ];
      const { handler, getMessages } = setup(initial);

      handler({
        type: AgentEventType.ToolCallUpdate,
        id: 'tool1',
        content: { type: ContentType.Text, text: 'line1\nline2\n' },
      });

      const msgs = getMessages();
      const toolMsg = msgs.find((m) => m.id === 'tool1');
      expect(toolMsg!.role).toBe(MessageRole.ToolUse);
      if (toolMsg!.role === MessageRole.ToolUse) {
        expect(toolMsg!.liveOutput).toEqual(['line1', 'line2']);
      }
    });

    it('does nothing when tool ID not found', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCallUpdate,
        id: 'nonexistent',
        content: { type: ContentType.Text, text: 'output' },
      });

      expect(getMessages()).toHaveLength(0);
    });
  });

  describe('ToolCallFinished events', () => {
    it('marks tool isFinished=true, sets result, clears liveOutput', () => {
      const initial: MessageType[] = [
        {
          id: 'tool1',
          role: MessageRole.ToolUse,
          name: 'shell',
          content: '{}',
          liveOutput: ['line1', 'line2'],
        } as MessageType,
      ];
      const { handler, getMessages } = setup(initial);

      handler({
        type: AgentEventType.ToolCallFinished,
        id: 'tool1',
        result: { status: 'success', output: 'done' },
      });

      const msgs = getMessages();
      const toolMsg = msgs.find((m) => m.id === 'tool1');
      expect(toolMsg!.role).toBe(MessageRole.ToolUse);
      if (toolMsg!.role === MessageRole.ToolUse) {
        expect(toolMsg!.isFinished).toBe(true);
        expect(toolMsg!.result).toEqual({ status: 'success', output: 'done' });
        expect(toolMsg!.liveOutput).toBeUndefined();
      }
    });

    it('does nothing when tool ID not found', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCallFinished,
        id: 'nonexistent',
        result: { status: 'success', output: '' },
      });

      expect(getMessages()).toHaveLength(0);
    });
  });

  describe('buildToolContent', () => {
    it('diff toolContent creates strReplace command', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'fs_write',
        args: { oldStr: 'old', newStr: 'new' },
        toolContent: [
          {
            type: 'diff' as const,
            path: '/test.txt',
            newText: 'new',
            oldText: 'old',
          },
        ],
      });

      const msgs = getMessages();
      const toolMsg = msgs.find((m) => m.id === 'tool1');
      const content = JSON.parse(toolMsg!.content);
      expect(content.command).toBe('strReplace');
      expect(content.path).toBe('/test.txt');
    });

    it('diff toolContent creates insert command', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'fs_write',
        args: { insertLine: 5 },
        toolContent: [
          {
            type: 'diff' as const,
            path: '/test.txt',
            newText: 'inserted',
          },
        ],
      });

      const msgs = getMessages();
      const toolMsg = msgs.find((m) => m.id === 'tool1');
      const content = JSON.parse(toolMsg!.content);
      expect(content.command).toBe('insert');
    });

    it('diff toolContent creates create command by default', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'fs_write',
        args: {},
        toolContent: [
          {
            type: 'diff' as const,
            path: '/new-file.txt',
            newText: 'content',
          },
        ],
      });

      const msgs = getMessages();
      const toolMsg = msgs.find((m) => m.id === 'tool1');
      const content = JSON.parse(toolMsg!.content);
      expect(content.command).toBe('create');
    });

    it('edit kind creates proper command JSON', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'fs_write',
        kind: 'edit',
        args: { path: '/test.txt', oldStr: 'old', newStr: 'new' },
      });

      const msgs = getMessages();
      const toolMsg = msgs.find((m) => m.id === 'tool1');
      const content = JSON.parse(toolMsg!.content);
      expect(content.command).toBe('strReplace');
      expect(content.path).toBe('/test.txt');
    });

    it('fallback is JSON.stringify(args)', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'grep',
        args: { query: 'test', path: '/src' },
      });

      const msgs = getMessages();
      const toolMsg = msgs.find((m) => m.id === 'tool1');
      const content = JSON.parse(toolMsg!.content);
      expect(content.query).toBe('test');
      expect(content.path).toBe('/src');
    });
  });
});
