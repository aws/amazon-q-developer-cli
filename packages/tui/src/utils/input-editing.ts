/**
 * Utility functions for text input editing operations (emacs/readline style)
 */

// Segment types (shared with PromptInput)
type TextSegment = { type: 'text'; value: string };
type FileSegment = {
  type: 'file';
  filePath: string;
  content: string;
  lineCount: number;
};
type PasteSegment = {
  type: 'paste';
  content: string;
  lineCount: number;
  charCount: number;
};
type ImageSegment = {
  type: 'image';
  /** Base64-encoded image data */
  base64: string;
  mimeType: string;
  width: number;
  height: number;
  sizeBytes: number;
};
export type Segment = TextSegment | FileSegment | PasteSegment | ImageSegment;

// Get cursor width of a segment (text = length, chip = 1)
export const segmentWidth = (s: Segment): number =>
  s.type === 'text' ? s.value.length : 1;

// Get total cursor width
export const totalWidth = (segments: Segment[]): number =>
  segments.reduce((sum, s) => sum + segmentWidth(s), 0);

// Get visible text for trigger detection
export const getVisibleText = (segments: Segment[]): string =>
  segments.map((s) => (s.type === 'text' ? s.value : '\u200B')).join('');

// Find segment and offset for cursor position
export const locateCursor = (
  segments: Segment[],
  cursor: number
): { segIdx: number; offset: number } => {
  if (segments.length === 0) return { segIdx: 0, offset: 0 };
  let pos = 0;
  for (let i = 0; i < segments.length; i++) {
    const w = segmentWidth(segments[i]!);
    if (cursor <= pos + w) {
      return { segIdx: i, offset: cursor - pos };
    }
    pos += w;
  }
  const lastSeg = segments[segments.length - 1]!;
  return { segIdx: segments.length - 1, offset: segmentWidth(lastSeg) };
};

// Normalize segments: merge adjacent text, remove empty text
export const normalizeSegments = (segments: Segment[]): Segment[] => {
  const result: Segment[] = [];
  for (const s of segments) {
    if (s.type === 'text') {
      if (s.value === '') continue;
      const last = result[result.length - 1];
      if (last?.type === 'text') {
        last.value += s.value;
      } else {
        result.push({ ...s });
      }
    } else {
      result.push(s);
    }
  }
  return result.length ? result : [{ type: 'text', value: '' }];
};

export interface EditResult {
  segments: Segment[];
  cursor: number;
}

/**
 * Delete word backward (Ctrl+W) - delete from cursor to previous word boundary
 */
export const deleteWordBackward = (
  segments: Segment[],
  cursor: number
): EditResult => {
  if (cursor === 0) return { segments, cursor };

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text') {
    const textBefore = seg.value.slice(0, offset);
    // Find word boundary - skip trailing spaces, then skip word chars
    let deleteStart = textBefore.length;
    // Skip spaces
    while (deleteStart > 0 && /\s/.test(textBefore[deleteStart - 1] ?? '')) {
      deleteStart--;
    }
    // Skip word characters
    while (deleteStart > 0 && /\S/.test(textBefore[deleteStart - 1] ?? '')) {
      deleteStart--;
    }
    const charsDeleted = offset - deleteStart;
    const newValue = seg.value.slice(0, deleteStart) + seg.value.slice(offset);
    const newSegs = [...segments];
    newSegs[segIdx] = { type: 'text', value: newValue };
    return {
      segments: normalizeSegments(newSegs),
      cursor: cursor - charsDeleted,
    };
  } else if (segIdx > 0) {
    // On a chip - delete the previous segment
    const prevSeg = segments[segIdx - 1];
    if (prevSeg) {
      const newSegs = [
        ...segments.slice(0, segIdx - 1),
        ...segments.slice(segIdx),
      ];
      return {
        segments: normalizeSegments(newSegs),
        cursor: cursor - segmentWidth(prevSeg),
      };
    }
  }
  return { segments, cursor };
};
/**
 * Delete character under cursor (Ctrl+D) - forward delete
 */
export const deleteForward = (
  segments: Segment[],
  cursor: number
): EditResult => {
  const total = totalWidth(segments);
  if (cursor >= total) return { segments, cursor };

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx]!;

  if (seg.type === 'text') {
    if (offset < seg.value.length) {
      // Delete char at cursor position
      const newValue = seg.value.slice(0, offset) + seg.value.slice(offset + 1);
      const newSegs = [...segments];
      newSegs[segIdx] = { type: 'text', value: newValue };
      return { segments: normalizeSegments(newSegs), cursor };
    }
    // At end of text segment — delete next segment (chip)
    if (segIdx + 1 < segments.length) {
      const newSegs = [
        ...segments.slice(0, segIdx + 1),
        ...segments.slice(segIdx + 2),
      ];
      return { segments: normalizeSegments(newSegs), cursor };
    }
  } else {
    // On a chip — delete it
    const newSegs = [
      ...segments.slice(0, segIdx),
      ...segments.slice(segIdx + 1),
    ];
    return { segments: normalizeSegments(newSegs), cursor };
  }

  return { segments, cursor };
};
/**
 * Delete word forward (Alt+D) - delete from cursor to next word boundary
 */
export const deleteWordForward = (
  segments: Segment[],
  cursor: number
): EditResult => {
  const total = totalWidth(segments);
  if (cursor >= total) return { segments, cursor };

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text') {
    const textAfter = seg.value.slice(offset);
    if (textAfter.length === 0 && segIdx + 1 < segments.length) {
      // At end of text segment — delete next segment (chip)
      const newSegs = [
        ...segments.slice(0, segIdx + 1),
        ...segments.slice(segIdx + 2),
      ];
      return { segments: normalizeSegments(newSegs), cursor };
    }
    // Find word boundary - skip word chars, then skip spaces
    let deleteEnd = 0;
    // Skip word characters
    while (
      deleteEnd < textAfter.length &&
      /\S/.test(textAfter[deleteEnd] ?? '')
    ) {
      deleteEnd++;
    }
    // Skip spaces
    while (
      deleteEnd < textAfter.length &&
      /\s/.test(textAfter[deleteEnd] ?? '')
    ) {
      deleteEnd++;
    }
    if (deleteEnd === 0) deleteEnd = 1; // At least delete one
    const newValue =
      seg.value.slice(0, offset) + seg.value.slice(offset + deleteEnd);
    const newSegs = [...segments];
    newSegs[segIdx] = { type: 'text', value: newValue };
    return {
      segments: normalizeSegments(newSegs),
      cursor,
    };
  } else {
    // On a chip - delete it
    const newSegs = [
      ...segments.slice(0, segIdx),
      ...segments.slice(segIdx + 1),
    ];
    return { segments: normalizeSegments(newSegs), cursor };
  }
};

/**
 * Kill to end of line (Ctrl+K)
 */
export const killToEnd = (segments: Segment[], cursor: number): EditResult => {
  const total = totalWidth(segments);
  if (cursor >= total) return { segments, cursor };

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text') {
    // Delete from cursor to end of this segment, plus all following segments
    const newValue = seg.value.slice(0, offset);
    const newSegs = [
      ...segments.slice(0, segIdx),
      { type: 'text' as const, value: newValue },
    ];
    return { segments: normalizeSegments(newSegs), cursor };
  } else {
    // On a chip - delete this and all following
    const newSegs = segments.slice(0, segIdx);
    return { segments: normalizeSegments(newSegs), cursor };
  }
};

/**
 * Kill to beginning of line (Ctrl+U)
 */
export const killToBeginning = (
  segments: Segment[],
  cursor: number
): EditResult => {
  if (cursor === 0) return { segments, cursor };

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text') {
    // Delete from start to cursor in this segment, plus all preceding segments
    const newValue = seg.value.slice(offset);
    const newSegs = [
      { type: 'text' as const, value: newValue },
      ...segments.slice(segIdx + 1),
    ];
    return { segments: normalizeSegments(newSegs), cursor: 0 };
  } else {
    // On a chip - delete all preceding segments
    const newSegs = segments.slice(segIdx);
    return { segments: normalizeSegments(newSegs), cursor: 0 };
  }
};

/**
 * Move forward one word (Alt+F / Ctrl+Right)
 */
export const moveWordForward = (
  segments: Segment[],
  cursor: number
): number => {
  const total = totalWidth(segments);
  if (cursor >= total) return cursor;

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text') {
    const textAfter = seg.value.slice(offset);
    // Skip current word chars, then skip spaces
    let moveBy = 0;
    // Skip word characters
    while (moveBy < textAfter.length && /\S/.test(textAfter[moveBy] ?? '')) {
      moveBy++;
    }
    // Skip spaces
    while (moveBy < textAfter.length && /\s/.test(textAfter[moveBy] ?? '')) {
      moveBy++;
    }
    if (moveBy === 0) moveBy = 1; // At least move one
    return Math.min(total, cursor + moveBy);
  } else {
    // On a chip - move past it
    return Math.min(total, cursor + 1);
  }
};

/**
 * Move backward one word (Alt+B / Ctrl+Left)
 */
export const moveWordBackward = (
  segments: Segment[],
  cursor: number
): number => {
  if (cursor === 0) return 0;

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text' && offset > 0) {
    const textBefore = seg.value.slice(0, offset);
    // Skip spaces, then skip word chars
    let moveBy = textBefore.length;
    // Skip spaces
    while (moveBy > 0 && /\s/.test(textBefore[moveBy - 1] ?? '')) {
      moveBy--;
    }
    // Skip word characters
    while (moveBy > 0 && /\S/.test(textBefore[moveBy - 1] ?? '')) {
      moveBy--;
    }
    const charsToMove = offset - moveBy;
    if (charsToMove === 0) {
      return Math.max(0, cursor - 1);
    }
    return cursor - charsToMove;
  } else if (segIdx > 0) {
    // At start of segment or on a chip - move to previous segment
    const prevSeg = segments[segIdx - 1];
    if (prevSeg) {
      return cursor - (offset > 0 ? offset : segmentWidth(prevSeg));
    }
  }
  return 0;
};

/**
 * Transpose characters (Ctrl+T)
 */
export const transposeChars = (
  segments: Segment[],
  cursor: number
): EditResult => {
  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text' && seg.value.length >= 2) {
    // If at end, swap last two chars; otherwise swap char before and at cursor
    let swapPos = offset;
    if (swapPos === 0) return { segments, cursor }; // Can't transpose at start
    if (swapPos >= seg.value.length) swapPos = seg.value.length - 1;

    const chars = seg.value.split('');
    const temp = chars[swapPos - 1];
    chars[swapPos - 1] = chars[swapPos] ?? '';
    chars[swapPos] = temp ?? '';

    const newSegs = [...segments];
    newSegs[segIdx] = { type: 'text', value: chars.join('') };

    // Move cursor forward (unless at end)
    const total = totalWidth(segments);
    const newCursor = cursor < total ? cursor + 1 : cursor;

    return { segments: newSegs, cursor: newCursor };
  }
  return { segments, cursor };
};

/**
 * Uppercase word (Alt+U) - uppercase from cursor to end of current word
 */
export const uppercaseWord = (
  segments: Segment[],
  cursor: number
): EditResult => {
  const total = totalWidth(segments);
  if (cursor >= total) return { segments, cursor };

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text') {
    const textAfter = seg.value.slice(offset);
    let end = 0;
    while (end < textAfter.length && /\S/.test(textAfter[end] ?? '')) {
      end++;
    }
    if (end === 0) {
      // Skip spaces first, then find word
      while (end < textAfter.length && /\s/.test(textAfter[end] ?? '')) {
        end++;
      }
      while (end < textAfter.length && /\S/.test(textAfter[end] ?? '')) {
        end++;
      }
    }
    if (end === 0) return { segments, cursor };
    const newValue =
      seg.value.slice(0, offset) +
      textAfter.slice(0, end).toUpperCase() +
      textAfter.slice(end);
    const newSegs = [...segments];
    newSegs[segIdx] = { type: 'text', value: newValue };
    return { segments: normalizeSegments(newSegs), cursor: cursor + end };
  }
  return { segments, cursor };
};

/**
 * Lowercase word (Alt+L) - lowercase from cursor to end of current word
 */
export const lowercaseWord = (
  segments: Segment[],
  cursor: number
): EditResult => {
  const total = totalWidth(segments);
  if (cursor >= total) return { segments, cursor };

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text') {
    const textAfter = seg.value.slice(offset);
    let end = 0;
    while (end < textAfter.length && /\S/.test(textAfter[end] ?? '')) {
      end++;
    }
    if (end === 0) {
      while (end < textAfter.length && /\s/.test(textAfter[end] ?? '')) {
        end++;
      }
      while (end < textAfter.length && /\S/.test(textAfter[end] ?? '')) {
        end++;
      }
    }
    if (end === 0) return { segments, cursor };
    const newValue =
      seg.value.slice(0, offset) +
      textAfter.slice(0, end).toLowerCase() +
      textAfter.slice(end);
    const newSegs = [...segments];
    newSegs[segIdx] = { type: 'text', value: newValue };
    return { segments: normalizeSegments(newSegs), cursor: cursor + end };
  }
  return { segments, cursor };
};

/**
 * Capitalize word (Alt+C) - capitalize first char, lowercase rest, from cursor to end of word
 */
export const capitalizeWord = (
  segments: Segment[],
  cursor: number
): EditResult => {
  const total = totalWidth(segments);
  if (cursor >= total) return { segments, cursor };

  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type === 'text') {
    const textAfter = seg.value.slice(offset);
    // Skip leading spaces
    let start = 0;
    while (start < textAfter.length && /\s/.test(textAfter[start] ?? '')) {
      start++;
    }
    let end = start;
    while (end < textAfter.length && /\S/.test(textAfter[end] ?? '')) {
      end++;
    }
    if (end === start) return { segments, cursor };
    const word = textAfter.slice(start, end);
    const capitalized = word[0]!.toUpperCase() + word.slice(1).toLowerCase();
    const newValue =
      seg.value.slice(0, offset) +
      textAfter.slice(0, start) +
      capitalized +
      textAfter.slice(end);
    const newSegs = [...segments];
    newSegs[segIdx] = { type: 'text', value: newValue };
    return { segments: normalizeSegments(newSegs), cursor: cursor + end };
  }
  return { segments, cursor };
};

/**
 * Transpose words (Alt+T) - swap the word before cursor with the word after cursor
 */
export const transposeWords = (
  segments: Segment[],
  cursor: number
): EditResult => {
  const { segIdx, offset } = locateCursor(segments, cursor);
  const seg = segments[segIdx];

  if (seg?.type !== 'text' || seg.value.length < 3) return { segments, cursor };

  const text = seg.value;

  // Find the word boundary around cursor
  // Find end of current/next word
  let rightEnd = offset;
  // Skip spaces after cursor
  while (rightEnd < text.length && /\s/.test(text[rightEnd] ?? '')) {
    rightEnd++;
  }
  // Find end of right word
  while (rightEnd < text.length && /\S/.test(text[rightEnd] ?? '')) {
    rightEnd++;
  }
  // Find start of right word
  let rightStart = rightEnd;
  while (rightStart > offset && /\S/.test(text[rightStart - 1] ?? '')) {
    rightStart--;
  }

  // Find the word before cursor
  let leftEnd = offset;
  // If cursor is in the middle of a word, leftEnd = start of that word's right boundary
  while (leftEnd > 0 && /\s/.test(text[leftEnd - 1] ?? '')) {
    leftEnd--;
  }
  let leftStart = leftEnd;
  while (leftStart > 0 && /\S/.test(text[leftStart - 1] ?? '')) {
    leftStart--;
  }

  // Need two distinct words with whitespace between them
  if (
    leftStart === leftEnd ||
    rightStart === rightEnd ||
    leftEnd >= rightStart
  ) {
    return { segments, cursor };
  }

  const leftWord = text.slice(leftStart, leftEnd);
  const middle = text.slice(leftEnd, rightStart);
  const rightWord = text.slice(rightStart, rightEnd);

  const newValue =
    text.slice(0, leftStart) +
    rightWord +
    middle +
    leftWord +
    text.slice(rightEnd);
  const newSegs = [...segments];
  newSegs[segIdx] = { type: 'text', value: newValue };
  return {
    segments: normalizeSegments(newSegs),
    cursor: cursor + (rightEnd - offset),
  };
};

/**
 * Check if the input content spans multiple lines (literal newlines only).
 */
export const isMultiLine = (segments: Segment[]): boolean => {
  return getVisibleText(segments).includes('\n');
};

/**
 * Check if the input visually spans multiple lines, considering both
 * literal newlines and visual wrapping at the given terminal width.
 */
export const isVisuallyMultiLine = (
  segments: Segment[],
  wrapWidth: number
): boolean => {
  if (wrapWidth <= 0) return isMultiLine(segments);
  const text = getVisibleText(segments);
  if (text.includes('\n')) return true;
  return text.length > wrapWidth;
};

/**
 * Get line info for cursor position in multi-line content.
 * Returns the line index, column, and array of line lengths.
 */
export const getCursorLineInfo = (
  segments: Segment[],
  cursor: number
): { lineIndex: number; col: number; lineLengths: number[] } => {
  const text = getVisibleText(segments);
  const lines = text.split('\n');
  const lineLengths = lines.map((l) => l.length);

  let remaining = cursor;
  for (let i = 0; i < lines.length; i++) {
    const lineLen = lineLengths[i]!;
    if (remaining <= lineLen) {
      return { lineIndex: i, col: remaining, lineLengths };
    }
    // +1 for the \n character
    remaining -= lineLen + 1;
  }
  // Fallback: cursor at end of last line
  return {
    lineIndex: lines.length - 1,
    col: lineLengths[lineLengths.length - 1] ?? 0,
    lineLengths,
  };
};

/**
 * Move cursor up one line in multi-line content.
 * Returns the new cursor position, or null if already on the first line.
 */
export const moveCursorUp = (
  segments: Segment[],
  cursor: number
): number | null => {
  const { lineIndex, col, lineLengths } = getCursorLineInfo(segments, cursor);
  if (lineIndex === 0) return null; // Already on first line

  const targetLine = lineIndex - 1;
  const targetCol = Math.min(col, lineLengths[targetLine]!);

  // Compute flat offset: sum of all lines before targetLine + their \n chars + targetCol
  let offset = 0;
  for (let i = 0; i < targetLine; i++) {
    offset += lineLengths[i]! + 1; // +1 for \n
  }
  offset += targetCol;
  return offset;
};

/**
 * Move cursor down one line in multi-line content.
 * Returns the new cursor position, or null if already on the last line.
 */
export const moveCursorDown = (
  segments: Segment[],
  cursor: number
): number | null => {
  const { lineIndex, col, lineLengths } = getCursorLineInfo(segments, cursor);
  if (lineIndex >= lineLengths.length - 1) return null; // Already on last line

  const targetLine = lineIndex + 1;
  const targetCol = Math.min(col, lineLengths[targetLine]!);

  let offset = 0;
  for (let i = 0; i < targetLine; i++) {
    offset += lineLengths[i]! + 1;
  }
  offset += targetCol;
  return offset;
};

/**
 * Split text into visual lines considering both literal newlines and
 * wrapping at the given width. Each visual line tracks its start offset
 * in the original flat text and its length.
 */
export const getVisualLines = (
  text: string,
  wrapWidth: number
): { start: number; length: number }[] => {
  if (wrapWidth <= 0) {
    // No wrapping — treat each literal line as a visual line
    const lines: { start: number; length: number }[] = [];
    let offset = 0;
    for (const line of text.split('\n')) {
      lines.push({ start: offset, length: line.length });
      offset += line.length + 1; // +1 for \n
    }
    return lines;
  }

  const lines: { start: number; length: number }[] = [];
  let offset = 0;

  for (const logicalLine of text.split('\n')) {
    if (logicalLine.length === 0) {
      // Empty line (e.g. consecutive \n)
      lines.push({ start: offset, length: 0 });
    } else {
      let remaining = logicalLine.length;
      let lineOffset = offset;
      while (remaining > 0) {
        const len = Math.min(remaining, wrapWidth);
        lines.push({ start: lineOffset, length: len });
        lineOffset += len;
        remaining -= len;
      }
    }
    offset += logicalLine.length + 1; // +1 for \n
  }

  return lines;
};

/**
 * Get visual line info for cursor position, accounting for wrapping.
 */
export const getVisualCursorLineInfo = (
  segments: Segment[],
  cursor: number,
  wrapWidth: number
): { lineIndex: number; col: number; lineCount: number } => {
  const text = getVisibleText(segments);
  const vlines = getVisualLines(text, wrapWidth);

  for (let i = 0; i < vlines.length; i++) {
    const vl = vlines[i]!;
    const lineEnd = vl.start + vl.length;
    // Cursor is on this visual line if it falls within [start, start+length]
    // For the last visual line of a logical line (before \n), the \n char
    // sits at lineEnd, so cursor === lineEnd belongs to this line unless
    // there's a next visual line starting at the same offset.
    if (cursor >= vl.start && cursor <= lineEnd) {
      // If cursor is exactly at lineEnd and the next visual line starts
      // at the same position (i.e. wrap boundary), cursor belongs to next line
      if (
        cursor === lineEnd &&
        i + 1 < vlines.length &&
        vlines[i + 1]!.start === lineEnd
      ) {
        continue;
      }
      return { lineIndex: i, col: cursor - vl.start, lineCount: vlines.length };
    }
  }

  // Fallback
  const lastLine = vlines[vlines.length - 1]!;
  return {
    lineIndex: vlines.length - 1,
    col: cursor - lastLine.start,
    lineCount: vlines.length,
  };
};

/**
 * Move cursor up one visual line (accounting for wrapping).
 * Returns the new cursor position, or null if already on the first visual line.
 */
export const moveCursorUpVisual = (
  segments: Segment[],
  cursor: number,
  wrapWidth: number
): number | null => {
  const text = getVisibleText(segments);
  const vlines = getVisualLines(text, wrapWidth);
  const { lineIndex, col } = getVisualCursorLineInfo(
    segments,
    cursor,
    wrapWidth
  );

  if (lineIndex === 0) return null;

  const targetLine = vlines[lineIndex - 1]!;
  const targetCol = Math.min(col, targetLine.length);
  return targetLine.start + targetCol;
};

/**
 * Move cursor down one visual line (accounting for wrapping).
 * Returns the new cursor position, or null if already on the last visual line.
 */
export const moveCursorDownVisual = (
  segments: Segment[],
  cursor: number,
  wrapWidth: number
): number | null => {
  const text = getVisibleText(segments);
  const vlines = getVisualLines(text, wrapWidth);
  const { lineIndex, col } = getVisualCursorLineInfo(
    segments,
    cursor,
    wrapWidth
  );

  if (lineIndex >= vlines.length - 1) return null;

  const targetLine = vlines[lineIndex + 1]!;
  const targetCol = Math.min(col, targetLine.length);
  return targetLine.start + targetCol;
};

/**
 * Move cursor to start of current visual line (Ctrl+A in multi-line).
 */
export const moveToVisualLineStart = (
  segments: Segment[],
  cursor: number,
  wrapWidth: number
): number => {
  const text = getVisibleText(segments);
  const vlines = getVisualLines(text, wrapWidth);
  const { lineIndex } = getVisualCursorLineInfo(segments, cursor, wrapWidth);
  return vlines[lineIndex]!.start;
};

/**
 * Move cursor to end of current visual line (Ctrl+E in multi-line).
 */
export const moveToVisualLineEnd = (
  segments: Segment[],
  cursor: number,
  wrapWidth: number
): number => {
  const text = getVisibleText(segments);
  const vlines = getVisualLines(text, wrapWidth);
  const { lineIndex } = getVisualCursorLineInfo(segments, cursor, wrapWidth);
  const vl = vlines[lineIndex]!;
  return vl.start + vl.length;
};

/**
 * Kill from cursor to end of current visual line (Ctrl+K in multi-line).
 */
export const killToVisualLineEnd = (
  segments: Segment[],
  cursor: number,
  wrapWidth: number
): EditResult => {
  const text = getVisibleText(segments);
  const vlines = getVisualLines(text, wrapWidth);
  const { lineIndex } = getVisualCursorLineInfo(segments, cursor, wrapWidth);
  const vl = vlines[lineIndex]!;
  const lineEnd = vl.start + vl.length;
  if (cursor >= lineEnd) return { segments, cursor };

  // Delete text from cursor to end of visual line in the flat text
  const newText = text.slice(0, cursor) + text.slice(lineEnd);
  return {
    segments: normalizeSegments([{ type: 'text', value: newText }]),
    cursor,
  };
};

/**
 * Kill from cursor to beginning of current visual line (Ctrl+U in multi-line).
 */
export const killToVisualLineBeginning = (
  segments: Segment[],
  cursor: number,
  wrapWidth: number
): EditResult => {
  const text = getVisibleText(segments);
  const vlines = getVisualLines(text, wrapWidth);
  const { lineIndex } = getVisualCursorLineInfo(segments, cursor, wrapWidth);
  const lineStart = vlines[lineIndex]!.start;
  if (cursor <= lineStart) return { segments, cursor };

  const newText = text.slice(0, lineStart) + text.slice(cursor);
  return {
    segments: normalizeSegments([{ type: 'text', value: newText }]),
    cursor: lineStart,
  };
};
