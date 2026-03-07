export interface MarkdownSegment {
  text: string;
  bold?: boolean;
  italic?: boolean;
  strikethrough?: boolean;
  quote?: boolean;
  blockquote?: boolean;
  header?: number;
  boldHeading?: boolean;
  horizontalRule?: boolean;
  link?: { url: string };
  listItem?: { ordered: boolean; number?: number; indent: number };
  codeBlock?: {
    code: string;
    language?: string;
    isComplete: boolean;
  };
  table?: {
    headers: string[];
    rows: string[][];
    alignments: ('left' | 'center' | 'right')[];
  };
}

enum State {
  TEXT,
  CODE_BLOCK,
}

const INLINE_CONTROL_MARKER_REGEX = /[`*_[\]~|>]/;
const BLOCK_CONTROL_MARKER_REGEX =
  /(^|\n)\s*(#{1,6}\s|[-+]\s|\d+\.\s|\|[^\n]*\||-{3,}|_{3,}|\*{3,})/;

interface CodeBlockTailContext {
  isInCodeBlock: boolean;
  inLanguageLine: boolean;
  languageLine?: string;
}

const getCodeBlockTailContext = (content: string): CodeBlockTailContext => {
  let state = State.TEXT;
  let inLanguageLine = false;
  let index = 0;
  let languageLineStart: number | null = null;

  while (index < content.length) {
    if (content.slice(index, index + 3) === '```') {
      if (state === State.TEXT) {
        state = State.CODE_BLOCK;
        inLanguageLine = true;

        index += 3;
        languageLineStart = index;
        while (index < content.length && content[index] !== '\n') {
          index++;
        }
        if (index < content.length && content[index] === '\n') {
          index++;
          inLanguageLine = false;
        }
        continue;
      }

      const afterFence = index + 3;
      const isClosingFence =
        afterFence >= content.length ||
        content[afterFence] === '\n' ||
        content[afterFence] === ' ';

      if (isClosingFence) {
        state = State.TEXT;
        inLanguageLine = false;
        languageLineStart = null;
        index += 3;
      } else {
        index += 3;
      }
      continue;
    }

    if (
      state === State.CODE_BLOCK &&
      inLanguageLine &&
      content[index] === '\n'
    ) {
      inLanguageLine = false;
    }
    index++;
  }

  return {
    isInCodeBlock: state === State.CODE_BLOCK,
    inLanguageLine: state === State.CODE_BLOCK && inLanguageLine,
    languageLine:
      state === State.CODE_BLOCK && inLanguageLine && languageLineStart !== null
        ? content.slice(languageLineStart)
        : undefined,
  };
};

const isIncrementalCodeBlockDeltaSafe = (delta: string): boolean =>
  !delta.includes('```');

const isPlainTextSegment = (segment: MarkdownSegment): boolean =>
  !!segment.text &&
  !segment.bold &&
  !segment.italic &&
  !segment.strikethrough &&
  !segment.quote &&
  !segment.blockquote &&
  !segment.header &&
  !segment.boldHeading &&
  !segment.horizontalRule &&
  !segment.link &&
  !segment.listItem &&
  !segment.codeBlock &&
  !segment.table;

const isBlockSeparatorSegment = (segment: MarkdownSegment): boolean =>
  !!segment.header ||
  !!segment.boldHeading ||
  !!segment.listItem ||
  !!segment.blockquote ||
  !!segment.horizontalRule ||
  !!segment.table ||
  (!!segment.codeBlock && segment.codeBlock.isComplete);

const hasAmbiguousTrailingBlockPrefix = (content: string): boolean => {
  const lastLineBreak = Math.max(
    content.lastIndexOf('\n'),
    content.lastIndexOf('\r')
  );
  const lastLine = content.slice(lastLineBreak + 1);
  return /^(?:\s*[-+*]\s*|\s*\d+\.\s*|\s*#{1,6}\s*|\s*>\s*)$/.test(lastLine);
};

export function isIncrementalMarkdownDeltaSafe(delta: string): boolean {
  if (!delta) {
    return true;
  }

  if (INLINE_CONTROL_MARKER_REGEX.test(delta)) {
    return false;
  }

  if (BLOCK_CONTROL_MARKER_REGEX.test(delta)) {
    return false;
  }

  return true;
}

/**
 * Incrementally append a streaming delta to an already parsed markdown segment list.
 *
 * Returns `null` when the delta may change markdown semantics and a full re-parse is required.
 */
export function tryAppendMarkdownDelta(
  previousSegments: MarkdownSegment[],
  delta: string,
  previousRawContent?: string
): MarkdownSegment[] | null {
  if (!delta) {
    return previousSegments;
  }

  const lastSegment = previousSegments[previousSegments.length - 1];
  const nextSegments = previousSegments.slice();

  if (lastSegment?.codeBlock && !lastSegment.codeBlock.isComplete) {
    if (!isIncrementalCodeBlockDeltaSafe(delta)) {
      return null;
    }

    if (previousRawContent) {
      const tailContext = getCodeBlockTailContext(previousRawContent);
      if (!tailContext.isInCodeBlock) {
        return null;
      }

      if (tailContext.inLanguageLine) {
        const newlineIndex = delta.indexOf('\n');
        const languageDelta =
          newlineIndex === -1 ? delta : delta.slice(0, newlineIndex);
        const codeDelta =
          newlineIndex === -1 ? '' : delta.slice(newlineIndex + 1);

        const languageLine = `${tailContext.languageLine || ''}${languageDelta}`;
        const updatedLanguage = languageLine.trim();
        const updatedLastSegment: MarkdownSegment = {
          ...lastSegment,
          codeBlock: {
            ...lastSegment.codeBlock,
            language: updatedLanguage || undefined,
            code: `${lastSegment.codeBlock.code}${codeDelta}`,
          },
        };
        nextSegments[nextSegments.length - 1] = updatedLastSegment;
        return nextSegments;
      }
    }

    const updatedLastSegment: MarkdownSegment = {
      ...lastSegment,
      codeBlock: {
        ...lastSegment.codeBlock,
        code: `${lastSegment.codeBlock.code}${delta}`,
      },
    };
    nextSegments[nextSegments.length - 1] = updatedLastSegment;
    return nextSegments;
  }

  if (
    lastSegment?.codeBlock?.isComplete &&
    previousRawContent?.endsWith('```') &&
    delta[0] !== '\n' &&
    delta[0] !== ' '
  ) {
    return null;
  }

  if (lastSegment?.table && !delta.startsWith('\n')) {
    return null;
  }

  if (lastSegment?.boldHeading && !delta.startsWith('\n')) {
    return null;
  }

  if (
    lastSegment &&
    (lastSegment.header || lastSegment.blockquote || lastSegment.listItem) &&
    !delta.startsWith('\n') &&
    delta.includes('\n')
  ) {
    return null;
  }

  if (
    lastSegment &&
    (lastSegment.header || lastSegment.blockquote || lastSegment.listItem) &&
    !(previousRawContent ? /\r?\n$/.test(previousRawContent) : false) &&
    !delta.includes('\n') &&
    isIncrementalMarkdownDeltaSafe(delta)
  ) {
    nextSegments[nextSegments.length - 1] = {
      ...lastSegment,
      text: `${lastSegment.text}${delta}`,
    };
    return nextSegments;
  }

  if (!isIncrementalMarkdownDeltaSafe(delta)) {
    return null;
  }

  let normalizedDelta = delta;
  if (!lastSegment || isBlockSeparatorSegment(lastSegment)) {
    normalizedDelta = normalizedDelta.replace(/^\n+/, '');
    if (!normalizedDelta) {
      return nextSegments;
    }
  }

  if (lastSegment && isPlainTextSegment(lastSegment)) {
    if (
      previousRawContent &&
      hasAmbiguousTrailingBlockPrefix(previousRawContent)
    ) {
      return null;
    }

    nextSegments[nextSegments.length - 1] = {
      ...lastSegment,
      text: `${lastSegment.text}${normalizedDelta}`,
    };
  } else {
    nextSegments.push({ text: normalizedDelta });
  }

  return nextSegments;
}

/**
 * Simple markdown processor for basic formatting
 * Supports: **bold**, *italic*, `code`, and ```language code blocks
 */
export const parseMarkdown = (text: string): MarkdownSegment[] => {
  const segments: MarkdownSegment[] = [];
  let state = State.TEXT;
  let currentText = '';
  let currentCode = '';
  let currentLanguage = '';

  const flushSegment = (isComplete = false) => {
    if (state === State.TEXT && currentText) {
      // Process line-by-line if there are headers, lists, bold headings, or blockquotes
      if (
        currentText.includes('#') ||
        currentText.includes('-') ||
        /^\s*\d+\./.test(currentText) ||
        currentText.includes('>') ||
        (currentText.includes('|') &&
          /^\s*\||\n[^\n]*\|[^\n]*\|/m.test(currentText)) ||
        /(?:^|\n)\*\*[^*]+\*\*\s*$/m.test(currentText)
      ) {
        const lines = currentText.split('\n');
        let textAccumulator = '';
        let tableLines: string[] = [];

        const flushText = () => {
          if (textAccumulator) {
            segments.push(...parseInlineMarkdown(textAccumulator));
            textAccumulator = '';
          }
        };

        const flushTable = () => {
          if (tableLines.length < 2) {
            for (const tl of tableLines) {
              if (textAccumulator) textAccumulator += '\n';
              textAccumulator += tl;
            }
            tableLines = [];
            return;
          }

          flushText();

          const parseRow = (line: string): string[] => {
            return line
              .split('|')
              .map((cell) => cell.trim())
              .filter((_, i, arr) => i > 0 && i < arr.length - 1);
          };

          const isSeparator = (line: string): boolean =>
            /^[\s|:-]+$/.test(line);

          let sepIdx = tableLines.findIndex((l) => isSeparator(l));
          if (sepIdx === -1) sepIdx = 1;

          const headers = parseRow(tableLines[0] || '');

          const sepCells = parseRow(tableLines[sepIdx] || '');
          const alignments = sepCells.map((cell) => {
            const trimmed = cell.replace(/\s/g, '');
            if (trimmed.startsWith(':') && trimmed.endsWith(':'))
              return 'center' as const;
            if (trimmed.endsWith(':')) return 'right' as const;
            return 'left' as const;
          });

          const rows = tableLines
            .filter((_, i) => i !== 0 && i !== sepIdx)
            .map(parseRow);

          segments.push({ text: '', table: { headers, rows, alignments } });
          tableLines = [];
        };

        for (const line of lines) {
          // Table detection: line must start with | or have | as column separators
          // (at least two | chars), not just any line containing |
          const isTableLine =
            /^\s*\|/.test(line) ||
            (line.split('|').length - 1 >= 2 && !/`[^`]*\|[^`]*`/.test(line));
          if (isTableLine) {
            tableLines.push(line);
            continue;
          } else if (tableLines.length > 0) {
            flushTable();
          }

          const headerMatch = line.match(/^(#{1,6})\s+(.+)$/);
          const boldHeadingMatch = line.match(/^\*\*([^*]+)\*\*\s*$/);
          const unorderedListMatch = line.match(/^(\s*)([-*+])\s+(.+)$/);
          const orderedListMatch = line.match(/^(\s*)(\d+)\.\s+(.+)$/);
          const blockquoteMatch = line.match(/^>\s?(.*)$/);
          const hrMatch = line.match(/^\s*(-{3,}|\*{3,}|_{3,})\s*$/);

          // Trim trailing newlines from accumulated text before block elements
          // \n\n before a block element is a markdown separator, not content
          const isBlockElement =
            headerMatch ||
            boldHeadingMatch ||
            unorderedListMatch ||
            orderedListMatch ||
            blockquoteMatch ||
            hrMatch;
          if (isBlockElement) {
            textAccumulator = textAccumulator.replace(/\n+$/, '');
          }

          if (headerMatch && headerMatch[1] && headerMatch[2]) {
            flushText();
            segments.push({
              text: headerMatch[2],
              header: headerMatch[1].length,
            });
          } else if (boldHeadingMatch && boldHeadingMatch[1]) {
            flushText();
            segments.push({
              text: boldHeadingMatch[1],
              boldHeading: true,
            });
          } else if (
            unorderedListMatch &&
            unorderedListMatch[1] !== undefined &&
            unorderedListMatch[3]
          ) {
            flushText();
            const indent = Math.floor(unorderedListMatch[1].length / 2);
            segments.push({
              text: unorderedListMatch[3],
              listItem: { ordered: false, indent },
            });
          } else if (
            orderedListMatch &&
            orderedListMatch[1] !== undefined &&
            orderedListMatch[2] &&
            orderedListMatch[3]
          ) {
            flushText();
            const indent = Math.floor(orderedListMatch[1].length / 2);
            const number = parseInt(orderedListMatch[2], 10);
            segments.push({
              text: orderedListMatch[3],
              listItem: { ordered: true, number, indent },
            });
          } else if (blockquoteMatch) {
            flushText();
            segments.push({
              text: blockquoteMatch[1] || '',
              blockquote: true,
            });
          } else if (hrMatch) {
            flushText();
            segments.push({ text: '', horizontalRule: true });
          } else {
            // Accumulate regular text with newlines preserved
            if (textAccumulator) textAccumulator += '\n';
            textAccumulator += line;
          }
        }
        if (tableLines.length > 0) flushTable();
        flushText();
      } else {
        segments.push(...parseInlineMarkdown(currentText));
      }
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

  // State machine: divide input into TEXT and CODE_BLOCK segments.
  // When we see a code fence like ```rust, we enter CODE_BLOCK mode
  // and remain there until we see the closing fence ```.
  // Everything in between is a code block.
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

        // If we see ``` followed by newline, space, or EOF, treat as closing
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

  // Post-pass: trim newlines at text↔code block boundaries and leading newlines
  for (let s = 0; s < segments.length; s++) {
    const seg = segments[s];
    if (
      seg &&
      seg.text &&
      !seg.codeBlock &&
      !seg.header &&
      !seg.boldHeading &&
      !seg.listItem &&
      !seg.blockquote &&
      !seg.horizontalRule &&
      !seg.table
    ) {
      // Trim leading newlines from the very first segment
      if (s === 0) {
        seg.text = seg.text.replace(/^\n+/, '');
      }
      // Trim trailing newlines before code blocks
      if (segments[s + 1]?.codeBlock) {
        seg.text = seg.text.replace(/\n+$/, '');
      }
      // Trim leading newlines after code blocks
      if (segments[s - 1]?.codeBlock) {
        seg.text = seg.text.replace(/^\n+/, '');
      }
    }
  }

  // Remove empty text segments created by trimming
  return segments.filter(
    (seg) =>
      seg.text ||
      seg.codeBlock ||
      seg.header ||
      seg.boldHeading ||
      seg.listItem ||
      seg.blockquote ||
      seg.horizontalRule ||
      seg.table
  );
};

/**
 * Parse inline markdown (bold, italic, code) in regular text
 */
export function parseInlineMarkdown(text: string): MarkdownSegment[] {
  const segments: MarkdownSegment[] = [];
  let remaining = text;

  const patterns = [
    {
      regex: /\[([^\]]+)\]\(([^)]+)\)/g,
      handler: (match: RegExpExecArray) => ({
        text: match[1] || '',
        link: { url: match[2] || '' },
      }),
    },
    {
      regex: /__(.*?)__/g,
      handler: (match: RegExpExecArray) => ({
        text: match[1] || '',
        bold: true,
      }),
    },
    {
      regex: /\*\*(.*?)\*\*/g,
      handler: (match: RegExpExecArray) => ({
        text: match[1] || '',
        bold: true,
      }),
    },
    {
      regex: /~~(.*?)~~/g,
      handler: (match: RegExpExecArray) => ({
        text: match[1] || '',
        strikethrough: true,
      }),
    },
    {
      regex: /_(.*?)_/g,
      handler: (match: RegExpExecArray) => ({
        text: match[1] || '',
        italic: true,
      }),
    },
    {
      regex: /\*(.*?)\*/g,
      handler: (match: RegExpExecArray) => ({
        text: match[1] || '',
        italic: true,
      }),
    },
    {
      regex: /`(.*?)`/g,
      handler: (match: RegExpExecArray) => ({
        text: match[1] || '',
        quote: true,
      }),
    },
  ];

  while (remaining.length > 0) {
    let earliestMatch = null;
    let earliestIndex = remaining.length;

    for (const pattern of patterns) {
      pattern.regex.lastIndex = 0;
      const match = pattern.regex.exec(remaining);
      if (match && match.index !== undefined && match.index < earliestIndex) {
        earliestMatch = { match, handler: pattern.handler };
        earliestIndex = match.index;
      }
    }

    if (earliestMatch) {
      if (earliestIndex > 0) {
        segments.push({ text: remaining.slice(0, earliestIndex) });
      }
      segments.push(earliestMatch.handler(earliestMatch.match));
      remaining = remaining.slice(
        earliestIndex + earliestMatch.match[0].length
      );
    } else {
      segments.push({ text: remaining });
      break;
    }
  }

  return segments;
}
