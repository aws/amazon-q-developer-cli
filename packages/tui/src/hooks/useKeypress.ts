import { useRef } from 'react';
import { useInput, usePaste } from './../renderer.js';
import { inputMetrics } from '../utils/inputMetrics.js';

export interface Key {
  upArrow: boolean;
  downArrow: boolean;
  leftArrow: boolean;
  rightArrow: boolean;
  pageUp: boolean;
  pageDown: boolean;
  home: boolean;
  end: boolean;
  return: boolean;
  escape: boolean;
  ctrl: boolean;
  shift: boolean;
  meta: boolean;
  tab: boolean;
  backspace: boolean;
  delete: boolean;
  paste?: boolean;
}

export type KeyHandler = (input: string, key: Key) => void;
export type EmptyPasteHandler = () => void;

/** Map a parsed Key event back to the raw bytes a terminal/PTY expects. */
export function keyToRawBytes(key: Key, userInput: string): string {
  if (key.return) return '\r';
  if (key.backspace) return '\x7f';
  if (key.tab) return '\t';
  if (key.escape) return '\x1b';
  if (key.upArrow) return '\x1b[A';
  if (key.downArrow) return '\x1b[B';
  if (key.rightArrow) return '\x1b[C';
  if (key.leftArrow) return '\x1b[D';
  if (key.delete) return '\x1b[3~';
  if (key.home) return '\x1b[H';
  if (key.end) return '\x1b[F';
  if (key.pageUp) return '\x1b[5~';
  if (key.pageDown) return '\x1b[6~';
  return userInput;
}
/**
 * Hook for handling keyboard input.
 *
 * Uses twinki's useInput for regular keys (proper React state batching)
 * and usePaste for bracketed paste detection.
 */
export const useKeypress = (
  handler: KeyHandler,
  options: { isActive?: boolean; onEmptyPaste?: EmptyPasteHandler } = {}
) => {
  const { isActive = true, onEmptyPaste } = options;

  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  const onEmptyPasteRef = useRef(onEmptyPaste);
  onEmptyPasteRef.current = onEmptyPaste;

  const isPastingRef = useRef(false);

  useInput(
    (input, key) => {
      if (isPastingRef.current) {
        return;
      }

      inputMetrics.markKeypress(input);
      inputMetrics.markHandlerStart();

      handlerRef.current(input, {
        ...key,
        meta: (key as any).meta ?? (key as any).alt ?? false,
        home: key.home ?? false,
        end: key.end ?? false,
        paste: false,
      });
    },
    { isActive }
  );

  usePaste(
    (content: string) => {
      isPastingRef.current = false;
      if (content) {
        handlerRef.current(content, {
          upArrow: false,
          downArrow: false,
          leftArrow: false,
          rightArrow: false,
          pageUp: false,
          pageDown: false,
          home: false,
          end: false,
          return: false,
          escape: false,
          ctrl: false,
          shift: false,
          meta: false,
          tab: false,
          backspace: false,
          delete: false,
          paste: true,
        });
      } else if (onEmptyPasteRef.current) {
        onEmptyPasteRef.current();
      }
    },
    { isActive }
  );
};
