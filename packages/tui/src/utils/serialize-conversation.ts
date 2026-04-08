import { MessageRole } from '../stores/app-store.js';

interface SerializableMessage {
  role: string;
  content: string;
}

/**
 * Serialize a conversation message list into a raw markdown string.
 *
 * User messages get a `## User` header, model messages get `## Kiro`.
 * Consecutive model messages (split by tool calls) are merged under one header.
 * Tool use and system messages are skipped.
 */
export function serializeConversationToMarkdown(
  messages: SerializableMessage[]
): string {
  const sections: string[] = [];
  let lastRole: 'user' | 'kiro' | null = null;
  let kiroBuffer: string[] = [];

  const flushKiro = () => {
    if (kiroBuffer.length > 0) {
      sections.push(`## Kiro\n\n${kiroBuffer.join('\n\n')}`);
      kiroBuffer = [];
    }
  };

  for (const msg of messages) {
    if (msg.role === MessageRole.User) {
      flushKiro();
      sections.push(`## User\n\n${msg.content}`);
      lastRole = 'user';
    } else if (msg.role === MessageRole.Model && msg.content) {
      if (lastRole !== 'kiro') flushKiro();
      kiroBuffer.push(msg.content);
      lastRole = 'kiro';
    }
    // ToolUse and System messages are skipped
  }

  flushKiro();
  return sections.join('\n\n');
}
