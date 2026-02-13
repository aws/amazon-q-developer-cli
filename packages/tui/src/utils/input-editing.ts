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
export type Segment = TextSegment | FileSegment | PasteSegment;

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
