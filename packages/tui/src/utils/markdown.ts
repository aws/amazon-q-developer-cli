export interface MarkdownSegment {
  text: string;
  bold?: boolean;
  italic?: boolean;
  quote?: boolean;
  codeBlock?: {
    code: string;
    language?: string;
    isComplete: boolean;
  };
}

enum State {
  TEXT,
  CODE_BLOCK,
}

/**
 * Simple markdown processor for basic formatting
 * Supports: **bold**, `code`, and ```language code blocks
 */
export const parseMarkdown = (text: string): MarkdownSegment[] => {
  const segments: MarkdownSegment[] = [];
  let state = State.TEXT;
  let currentText = '';
  let currentCode = '';
  let currentLanguage = '';

  const flushSegment = (isComplete = false) => {
    if (state === State.TEXT && currentText) {
      segments.push(...parseInlineMarkdown(currentText));
      currentText = '';
    } else if (state === State.CODE_BLOCK) {
      segments.push({
        text: '',
        codeBlock: {
          code: currentCode,
          language: currentLanguage || undefined,
          isComplete: isComplete,
        },
      });
      currentCode = '';
      currentLanguage = '';
    }
  };

  // simple state machine - the goal is to divide the input into TEXT and CODE blocks
  // the idea is if we see code fence like ```rust, we enter CODE_BLOCK mode. and we remain in this mode until we see the closing fence ```
  // everything in between is code block.
  let i = 0;
  while (i < text.length) {
    // Check for triple backticks
    if (text.slice(i, i + 3) === '```') {
      if (state === State.TEXT) {
        // Entering code block - flush current text
        flushSegment();

        // Extract language (everything until newline)
        i += 3; // skip ```
        const lineStart = i;
        while (i < text.length && text[i] !== '\n') {
          i++;
        }
        currentLanguage = text.slice(lineStart, i).trim();
        if (i < text.length) i++; // skip newline

        state = State.CODE_BLOCK;
      } else if (state === State.CODE_BLOCK) {
        // Check if this is closing fence
        const afterFence = i + 3;

        // if we see ``` followed by new line or space, or EOF, then we treat it as closing the code block
        const isClosingFence =
          afterFence >= text.length ||
          text[afterFence] === '\n' ||
          text[afterFence] === ' ';

        if (isClosingFence) {
          // Exiting code block - flush as complete
          flushSegment(true);
          state = State.TEXT;
          i += 3; // skip closing ```
        } else {
          // It's just content with ``` in it
          currentCode += text.slice(i, i + 3);
          i += 3;
        }
      }
    } else {
      // Regular character
      if (state === State.TEXT) {
        currentText += text[i];
      } else {
        currentCode += text[i];
      }
      i++;
    }
  }

  // Handle remaining content
  flushSegment();

  return segments;
};

/**
 * Parse inline markdown (bold, italic, code) in regular text
 */
function parseInlineMarkdown(text: string): MarkdownSegment[] {
  const segments: MarkdownSegment[] = [];
  let remaining = text;

  // Find markdown patterns and split text
  const patterns = [
    { regex: /\*\*(.*?)\*\*/g, style: { bold: true } },
    { regex: /\*(.*?)\*/g, style: { italic: true } },
    { regex: /`(.*?)`/g, style: { quote: true } },
  ];

  while (remaining.length > 0) {
    let earliestMatch = null;
    let earliestIndex = remaining.length;

    // Find the earliest markdown pattern
    for (const pattern of patterns) {
      pattern.regex.lastIndex = 0;
      const match = pattern.regex.exec(remaining);
      if (match && match.index !== undefined && match.index < earliestIndex) {
        earliestMatch = { match, style: pattern.style };
        earliestIndex = match.index;
      }
    }

    if (earliestMatch) {
      // Add text before match
      if (earliestIndex > 0) {
        segments.push({ text: remaining.slice(0, earliestIndex) });
      }
      // Add styled text
      segments.push({
        text: earliestMatch.match[1] ?? '',
        ...earliestMatch.style,
      });
      // Continue with remaining text
      remaining = remaining.slice(
        earliestIndex + earliestMatch.match[0].length
      );
    } else {
      // No more matches, add remaining text
      segments.push({ text: remaining });
      break;
    }
  }

  return segments;
}
