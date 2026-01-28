/**
 * Represents the minimum state required for the input prompt.
 *
 * Mostly copied from Gemini CLI.
 */
export interface InputBufferState {
  lines: string[];
  cursorRow: number;
  cursorCol: number;
  /**
   * When the user moves the caret vertically we try to keep their original
   * horizontal column even when passing through shorter lines.  We remember
   * that *preferred* column in this field while the user is still travelling
   * vertically.  Any explicit horizontal movement resets the preference.
   */
  preferredCursorCol: number;
  undoStack: UndoHistoryEntry[];
  redoStack: UndoHistoryEntry[];
  viewportWidth: number;
  viewportHeight: number;
  /**
   * The lines to render.
   */
  visibleLines: string[];
  // For each logical line, an array of [visualLineIndex, startColInLogical]
  logicalToVisibleMap: Array<Array<[number, number]>>;
  // For each visual line, its [logicalLineIndex, startColInLogical]
  visibleToLogicalMap: Array<[number, number]>;
}

/**
 * Methods for updating `InputBufferState`
 */
export interface InputBufferActions {
  insert: (char: string) => void;
  newline: () => void;
  backspace: () => void;
  delete: () => void;
  clearWord: () => void;
  clearLine: () => void;
  clearInput: () => void;
  moveCursor: (dir: MoveCursorDir) => void;
  setViewport: (width: number, height: number) => void;
}

export interface MemoizedInputBufferState {
  text: string;
}

export type MoveCursorDir =
  | 'left'
  | 'right'
  | 'up'
  | 'down'
  | 'wordLeft'
  | 'wordRight'
  | 'home'
  | 'end';

export interface UndoHistoryEntry {
  lines: string[];
  cursorRow: number;
  cursorCol: number;
}

export type InputBuffer = InputBufferState &
  MemoizedInputBufferState &
  InputBufferActions;

export interface UseInputBufferProps {
  viewportWidth: number;
  viewportHeight: number;
}
