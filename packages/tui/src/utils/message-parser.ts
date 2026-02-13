import { highlight } from 'cli-highlight';

export interface ContentBlock {
  type: 'text' | 'code' | 'tool_call' | 'tool_output';
  content: string;
  language?: string;
  metadata?: Record<string, any>;
}

interface ParseState {
  blocks: ContentBlock[];
  currentBlock: string;
  inCodeBlock: boolean;
  codeLanguage?: string;
}

export function parseMarkdownChunk(chunk: string): ContentBlock[] {
  // For streaming, we'll do simple parsing
  // Look for code block markers
  const lines = chunk.split('\n');
  const blocks: ContentBlock[] = [];

  for (const line of lines) {
    if (line.startsWith('```')) {
      // Code block start/end
      const language = line.slice(3).trim();
      if (language) {
        // Start of code block
        blocks.push({
          type: 'code',
          content: '',
          language,
        });
      } else {
        // End of code block - handled by complete parsing
        blocks.push({
          type: 'text',
          content: line + '\n',
        });
      }
    } else {
      // Regular text
      blocks.push({
        type: 'text',
        content: line + (lines.length > 1 ? '\n' : ''),
      });
    }
  }

  return blocks;
}

export function parseMarkdownComplete(content: string): ContentBlock[] {
  const blocks: ContentBlock[] = [];
  const lines = content.split('\n');
  let i = 0;

  while (i < lines.length) {
    const line = lines[i]!;

    if (line.startsWith('```')) {
      // Code block
      const language = line.slice(3).trim() || 'text';
      i++; // Skip opening ```

      const codeLines: string[] = [];
      while (i < lines.length && !lines[i]!.startsWith('```')) {
        codeLines.push(lines[i]!);
        i++;
      }

      const codeContent = codeLines.join('\n');
      blocks.push({
        type: 'code',
        content: codeContent,
        language,
      });

      i++; // Skip closing ```
    } else if (line.includes('Tool Call:') || line.includes('TOOL_CALL:')) {
      // Tool call detection
      blocks.push({
        type: 'tool_call',
        content: line,
      });
      i++;
    } else {
      // Regular text - accumulate consecutive text lines
      const textLines: string[] = [];
      while (
        i < lines.length &&
        !lines[i]!.startsWith('```') &&
        !lines[i]!.includes('Tool Call:') &&
        !lines[i]!.includes('TOOL_CALL:')
      ) {
        textLines.push(lines[i]!);
        i++;
      }

      if (textLines.length > 0) {
        blocks.push({
          type: 'text',
          content: textLines.join('\n'),
        });
      }
    }
  }

  return blocks;
}

export function renderContentBlock(block: ContentBlock): string {
  switch (block.type) {
    case 'code':
      try {
        // Use cli-highlight for syntax highlighting
        return highlight(block.content, {
          language: block.language || 'text',
          theme: 'default',
        });
      } catch (error) {
        // Fallback to plain text if highlighting fails
        return block.content;
      }

    case 'tool_call':
      return `🔧 ${block.content}`;

    case 'tool_output':
      return `📤 ${block.content}`;

    case 'text':
    default:
      return block.content;
  }
}
