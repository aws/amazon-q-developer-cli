import { describe, it, expect, mock, afterAll } from 'bun:test';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({})),
}));

afterAll(() => {
  mock.restore();
});

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
  describe('user turn boundaries', () => {
    it('preserves protocol user messages before subsequent reasoning', async () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.UserMessage,
        id: 'user-1',
        content: { type: ContentType.Text, text: 'tighten the validation' },
      });
      handler({
        type: AgentEventType.Thought,
        id: 'thought-1',
        content: { type: ContentType.Text, text: 'Reviewing the evidence.' },
      });

      await new Promise((resolve) => setTimeout(resolve, 50));

      expect(getMessages()).toEqual([
        {
          id: 'user-1',
          role: MessageRole.User,
          content: 'tighten the validation',
          agentName: 'test-agent',
        },
        {
          id: 'thought-1',
          role: MessageRole.Model,
          content: '',
          thinking: 'Reviewing the evidence.',
          agentName: 'test-agent',
        },
      ]);
    });

    it('renders one consumed steering message when delivery repeats', () => {
      const { handler, getMessages } = setup();
      const event = {
        type: AgentEventType.SteeringConsumed,
        content: 'focus on the failed assertion',
      } as const;

      handler(event);
      handler(event);

      expect(getMessages()).toHaveLength(1);
      expect(getMessages()[0]).toMatchObject({
        role: MessageRole.User,
        content: 'focus on the failed assertion',
        steered: true,
      });
    });
  });

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

    it('never emits history-only cancellation placeholders', async () => {
      for (const suffix of ['', '\n', '\r\n']) {
        const { handler, getMessages } = setup();
        for (const text of [
          'Response was ',
          `interrupted by the user${suffix}`,
        ]) {
          handler({
            type: AgentEventType.Content,
            id: 'c1',
            content: { type: ContentType.Text, text },
          });
          await new Promise((r) => setTimeout(r, 25));
          expect(getMessages()).toHaveLength(0);
        }
      }
    });

    it('releases a held placeholder prefix when normal content diverges', async () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: {
          type: ContentType.Text,
          text: 'Response was interrupted by the user',
        },
      });
      await new Promise((r) => setTimeout(r, 25));
      expect(getMessages()).toHaveLength(0);

      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: { type: ContentType.Text, text: ', so I restarted it.' },
      });
      await new Promise((r) => setTimeout(r, 25));
      expect(getMessages()[0]?.content).toBe(
        'Response was interrupted by the user, so I restarted it.'
      );
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

    it('propagates provenance when a full MCP event replaces a placeholder', () => {
      const initial: MessageType[] = [
        {
          id: 'tool-mcp',
          role: MessageRole.ToolUse,
          name: 'fs_read',
          content: '{}',
        },
      ];
      const { handler, getMessages } = setup(initial);

      handler({
        type: AgentEventType.ToolCall,
        id: 'tool-mcp',
        name: 'fs_read',
        origin: 'mcp',
        originalTitle: '@server/fs_read',
        kind: 'read',
        args: { customPath: '/not-a-builtin-shape' },
      });

      const toolMsg = getMessages().find((m) => m.id === 'tool-mcp');
      expect(toolMsg).toMatchObject({
        origin: 'mcp',
        originalTitle: '@server/fs_read',
      });
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
        expect(toolMsg!.liveOutput).toBeUndefined();
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
        expect(toolMsg!.liveOutput).toEqual(['line1', 'line2']);
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

  describe('diff extraction (deriveToolDiff)', () => {
    it('toolContent populates msg.diff with strReplace-style fields', () => {
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
      expect(toolMsg!.role).toBe(MessageRole.ToolUse);
      if (toolMsg!.role === MessageRole.ToolUse) {
        expect(toolMsg!.diff).toEqual({
          path: '/test.txt',
          newText: 'new',
          oldText: 'old',
        });
        // content stays as a JSON-encoded copy of the args, no diff fields
        // re-stringified into it.
        expect(JSON.parse(toolMsg!.content)).toEqual({
          oldStr: 'old',
          newStr: 'new',
        });
      }
    });

    it('toolContent populates msg.diff for insert-style writes', () => {
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
      if (toolMsg!.role === MessageRole.ToolUse) {
        expect(toolMsg!.diff).toEqual({
          path: '/test.txt',
          newText: 'inserted',
          oldText: undefined,
        });
      }
    });

    it('toolContent populates msg.diff for create-style writes', () => {
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
      if (toolMsg!.role === MessageRole.ToolUse) {
        expect(toolMsg!.diff).toEqual({
          path: '/new-file.txt',
          newText: 'content',
          oldText: undefined,
        });
      }
    });

    it('edit kind without toolContent synthesizes diff from args', () => {
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
      if (toolMsg!.role === MessageRole.ToolUse) {
        expect(toolMsg!.diff).toEqual({
          path: '/test.txt',
          newText: 'new',
          oldText: 'old',
        });
      }
    });

    it('non-edit tools leave msg.diff undefined; content holds args', () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'grep',
        args: { query: 'test', path: '/src' },
      });

      const msgs = getMessages();
      const toolMsg = msgs.find((m) => m.id === 'tool1');
      if (toolMsg!.role === MessageRole.ToolUse) {
        expect(toolMsg!.diff).toBeUndefined();
        const content = JSON.parse(toolMsg!.content);
        expect(content.query).toBe('test');
        expect(content.path).toBe('/src');
      }
    });
  });

  describe('Thought events', () => {
    it('buffers thinking text and flushes with thinking field', async () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.Thought,
        id: 't1',
        content: { type: ContentType.Text, text: 'Let me think...' },
      });

      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      expect(msgs).toHaveLength(1);
      expect(msgs[0]!.role).toBe(MessageRole.Model);
      expect(msgs[0]!.content).toBe('');
      expect((msgs[0] as any).thinking).toBe('Let me think...');
    });

    it('accumulates thinking across multiple chunks', async () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.Thought,
        id: 't1',
        content: { type: ContentType.Text, text: 'First thought. ' },
      });
      handler({
        type: AgentEventType.Thought,
        id: 't1',
        content: { type: ContentType.Text, text: 'Second thought.' },
      });

      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      expect(msgs).toHaveLength(1);
      expect((msgs[0] as any).thinking).toBe('First thought. Second thought.');
    });

    it('coexists with content events on the same message', async () => {
      const { handler, getMessages } = setup();

      handler({
        type: AgentEventType.Thought,
        id: 't1',
        content: { type: ContentType.Text, text: 'thinking...' },
      });
      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: { type: ContentType.Text, text: 'Here is my answer.' },
      });

      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      expect(msgs).toHaveLength(1);
      expect(msgs[0]!.role).toBe(MessageRole.Model);
      expect(msgs[0]!.content).toBe('Here is my answer.');
      expect((msgs[0] as any).thinking).toBe('thinking...');
    });

    it('preserves thinking when updating existing model message', async () => {
      const initial: MessageType[] = [
        {
          id: 'm1',
          role: MessageRole.Model,
          content: '',
          thinking: 'prior thought',
        },
      ];
      const { handler, getMessages } = setup(initial);

      handler({
        type: AgentEventType.Content,
        id: 'm1',
        content: { type: ContentType.Text, text: 'response text' },
      });

      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      expect(msgs).toHaveLength(1);
      expect(msgs[0]!.content).toBe('response text');
      expect((msgs[0] as any).thinking).toBe('prior thought');
    });

    it('clears thinking buffer when a ToolCall arrives', async () => {
      const { handler, getMessages } = setup();

      // Buffer thinking text
      handler({
        type: AgentEventType.Thought,
        id: 't1',
        content: { type: ContentType.Text, text: 'old thinking' },
      });

      // ToolCall should flush and clear thinking
      handler({
        type: AgentEventType.ToolCall,
        id: 'tool1',
        name: 'fs_read',
        args: { path: '/test.txt' },
      });

      // Now send new content for the post-tool model message
      handler({
        type: AgentEventType.Content,
        id: 'c1',
        content: { type: ContentType.Text, text: 'after tool' },
      });

      await new Promise((r) => setTimeout(r, 50));

      const msgs = getMessages();
      const modelMsgs = msgs.filter((m) => m.role === MessageRole.Model);
      // The post-tool model message should NOT carry the old thinking
      const lastModel = modelMsgs[modelMsgs.length - 1];
      expect(lastModel).toBeDefined();
      expect(lastModel!.content).toBe('after tool');
      expect((lastModel as any).thinking).toBeUndefined();
    });
  });
});
