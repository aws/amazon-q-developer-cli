import { useEffect, useRef } from 'react';
import { useInput, useStdin } from 'ink';
import { logger } from '../utils/logger.js';
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

// Bracketed paste escape sequences
const PASTE_START = '\x1b[200~';
const PASTE_END = '\x1b[201~';

// Ctrl+C byte
const CTRL_C = '\x03';

/**
 * Hook for handling keyboard input.
 *
 * Uses Ink's useInput for regular keys (proper React state batching via reconciler.batchedUpdates)
 * and listens to the internal event emitter for:
 * - Bracketed paste detection (useInput doesn't preserve paste sequences)
 * - Multiple Ctrl+C handling (when sent as a single chunk like \x03\x03)
 *
 * Why this hybrid approach:
 * - useInput wraps handlers in batchedUpdates(), preventing stale closure issues when
 *   multiple keys arrive in a single chunk (e.g., fast typing or test automation)
 * - Direct event listening is needed for paste detection and handling multiple
 *   Ctrl+C bytes in a single chunk (common in test automation)
 */
export const useKeypress = (
  handler: KeyHandler,
  options: { isActive?: boolean; onEmptyPaste?: EmptyPasteHandler } = {}
) => {
  const { isActive = true, onEmptyPaste } = options;

  // Store handler in ref to always call latest version
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  // Store onEmptyPaste in ref for stable access
  const onEmptyPasteRef = useRef(onEmptyPaste);
  onEmptyPasteRef.current = onEmptyPaste;

  // Access internal event emitter for paste detection and multi-Ctrl+C handling
  const { internal_eventEmitter } = useStdin() as {
    internal_eventEmitter: import('events').EventEmitter;
  };

  // Track whether we're currently in a paste operation to prevent useInput from processing paste content
  const isPastingRef = useRef(false);

  // Use Ink's useInput for regular keys - it handles batching properly
  // Skip processing when we're in the middle of a bracketed paste
  useInput(
    (input, key) => {
      if (isPastingRef.current) {
        return;
      }

      inputMetrics.markKeypress(input);
      inputMetrics.markHandlerStart();

      handlerRef.current(input, {
        ...key,
        home: key.home ?? false,
        end: key.end ?? false,
        paste: false,
      });
    },
    { isActive }
  );

  // Listen for special sequences on the internal event emitter
  useEffect(() => {
    if (!isActive || !internal_eventEmitter) return;

    let pasteBuffer = '';
    let isPasting = false;

    const handleInput = (data: string) => {
      // Check for paste start
      if (data.includes(PASTE_START)) {
        isPasting = true;
        isPastingRef.current = true;
        pasteBuffer = '';

        // Extract content after paste start marker
        const startIdx = data.indexOf(PASTE_START) + PASTE_START.length;
        const remaining = data.slice(startIdx);

        // Check if paste end is in the same chunk
        if (remaining.includes(PASTE_END)) {
          const endIdx = remaining.indexOf(PASTE_END);
          const pastedContent = remaining.slice(0, endIdx);
          isPasting = false;

          // Keep isPastingRef true until after this event loop tick.
          // Ink's useInput handler also listens on the same 'input' event
          // and fires for the same chunk. If we clear isPastingRef
          // synchronously, useInput sees isPastingRef=false and lets
          // the paste-end sequence (e.g. "[201~") leak through as
          // printable text. Deferring the reset ensures useInput still
          // skips this chunk.
          queueMicrotask(() => {
            isPastingRef.current = false;
          });

          if (pastedContent) {
            handlerRef.current(pastedContent, {
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
            // Empty bracketed paste — clipboard has no text (likely image data).
            // Notify the caller so it can attempt an image paste.
            onEmptyPasteRef.current();
          }
        } else {
          pasteBuffer = remaining;
        }
        return;
      }

      // Continue collecting paste content
      if (isPasting) {
        if (data.includes(PASTE_END)) {
          const endIdx = data.indexOf(PASTE_END);
          pasteBuffer += data.slice(0, endIdx);
          isPasting = false;

          // Defer clearing isPastingRef so Ink's useInput handler
          // (which fires for the same 'input' event) still sees the
          // pasting flag and skips this chunk. See comment above.
          queueMicrotask(() => {
            isPastingRef.current = false;
          });

          if (pasteBuffer) {
            handlerRef.current(pasteBuffer, {
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
            // Empty bracketed paste (multi-chunk) — try image paste
            onEmptyPasteRef.current();
          }
          pasteBuffer = '';
        } else {
          pasteBuffer += data;
        }
        return;
      }

      // Handle multiple Ctrl+C bytes in a single chunk
      // useInput's parseKeypress only handles single-character input, so when multiple
      // Ctrl+C bytes arrive together (e.g., [0x03, 0x03]), none get processed by useInput.
      // We need to handle ALL of them here.
      if (data.length > 1 && data.includes(CTRL_C)) {
        // eslint-disable-next-line no-control-regex
        const ctrlCCount = (data.match(/\x03/g) || []).length;
        // Debug log
        logger.debug(
          `[useKeypress] Multiple Ctrl+C detected: ${ctrlCCount} in chunk of length ${data.length}`
        );
        // Fire all Ctrl+C events since useInput won't handle any of them
        for (let i = 0; i < ctrlCCount; i++) {
          handlerRef.current('c', {
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
            ctrl: true,
            shift: false,
            meta: false,
            tab: false,
            backspace: false,
            delete: false,
            paste: false,
          });
        }
      }
    };

    internal_eventEmitter.on('input', handleInput);
    return () => {
      internal_eventEmitter.removeListener('input', handleInput);
      isPastingRef.current = false;
    };
  }, [isActive, internal_eventEmitter]);
};
