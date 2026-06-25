import { Lexer, type Token, type Tokens } from 'marked';
import { MessageRole } from '../stores/app-store.js';

export interface SerializableMessage {
  role: string;
  content: string;
}

export type TranscriptFormat = 'markdown' | 'plaintext' | 'json';

/**
 * Serialize a conversation to the specified format.
 */
export function serializeConversation(
  messages: SerializableMessage[],
  format: TranscriptFormat = 'markdown'
): string {
  switch (format) {
    case 'json':
      return JSON.stringify(
        messages
          .filter(
            (m) =>
              (m.role === MessageRole.User || m.role === MessageRole.Model) &&
              m.content
          )
          .map((m) => ({ role: m.role, content: m.content })),
        null,
        2
      );
    case 'plaintext':
      return serializeWithLabels(messages, 'User:', 'Kiro:', stripMarkdown);
    case 'markdown':
    default:
      return serializeWithLabels(messages, '## User', '## Kiro');
  }
}

// Keep the original export for backward compatibility
export function serializeConversationToMarkdown(
  messages: SerializableMessage[]
): string {
  return serializeConversation(messages, 'markdown');
}

/**
 * Serialize messages with configurable labels and optional content transform.
 */
function serializeWithLabels(
  messages: SerializableMessage[],
  userLabel: string,
  kiroLabel: string,
  transform?: (text: string) => string
): string {
  const sections: string[] = [];
  let lastRole: 'user' | 'kiro' | null = null;
  let kiroBuffer: string[] = [];

  const flushKiro = () => {
    if (kiroBuffer.length > 0) {
      sections.push(`${kiroLabel}\n\n${kiroBuffer.join('\n\n')}`);
      kiroBuffer = [];
    }
  };

  for (const msg of messages) {
    if (msg.role === MessageRole.User) {
      flushKiro();
      const content = transform ? transform(msg.content) : msg.content;
      sections.push(`${userLabel}\n\n${content}`);
      lastRole = 'user';
    } else if (msg.role === MessageRole.Model && msg.content) {
      if (lastRole !== 'kiro') flushKiro();
      kiroBuffer.push(transform ? transform(msg.content) : msg.content);
      lastRole = 'kiro';
    }
  }

  flushKiro();
  return sections.join('\n\n');
}

/**
 * Strip markdown formatting using marked's lexer to extract plain text.
 */
function stripMarkdown(text: string): string {
  const tokens = Lexer.lex(text);
  return extractText(tokens)
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

function extractText(tokens: Token[]): string {
  const parts: string[] = [];

  for (const token of tokens) {
    switch (token.type) {
      case 'heading':
      case 'paragraph':
        parts.push(
          extractInline(
            (token as Tokens.Heading | Tokens.Paragraph).tokens ?? []
          ) + '\n\n'
        );
        break;
      case 'code':
        parts.push((token as Tokens.Code).text + '\n\n');
        break;
      case 'blockquote':
        parts.push(extractText((token as Tokens.Blockquote).tokens ?? []));
        break;
      case 'list':
        for (const item of (token as Tokens.List).items) {
          parts.push('- ' + extractInline(item.tokens ?? []).trim() + '\n');
        }
        parts.push('\n');
        break;
      case 'table': {
        const table = token as Tokens.Table;
        const headers = table.header.map((h) => extractInline(h.tokens ?? []));
        parts.push(headers.join('\t') + '\n');
        for (const row of table.rows) {
          parts.push(
            row.map((cell) => extractInline(cell.tokens ?? [])).join('\t') +
              '\n'
          );
        }
        parts.push('\n');
        break;
      }
      case 'space':
      case 'hr':
        parts.push('\n');
        break;
      default:
        if ('tokens' in token && Array.isArray(token.tokens)) {
          parts.push(extractText(token.tokens));
        } else if ('text' in token) {
          parts.push((token as Tokens.Text).text);
        }
    }
  }

  return parts.join('');
}

function extractInline(tokens: Token[]): string {
  return tokens
    .map((t) => {
      if ('tokens' in t && Array.isArray(t.tokens)) {
        return extractInline(t.tokens);
      }
      if (t.type === 'codespan') return (t as Tokens.Codespan).text;
      if (t.type === 'image') return (t as Tokens.Image).text;
      if (t.type === 'br') return '\n';
      if ('text' in t) return (t as Tokens.Text).text;
      return '';
    })
    .join('');
}
