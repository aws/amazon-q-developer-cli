import { useEffect, useRef } from 'react';
import { useInput, useStdin } from 'ink';
import { logger } from '../utils/logger.js';

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
  options: { isActive?: boolean } = {}
) => {
  const { isActive = true } = options;
  
  // Store handler in ref to always call latest version
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  // Access internal event emitter for paste detection and multi-Ctrl+C handling
  const { internal_eventEmitter } = useStdin() as { internal_eventEmitter: import('events').EventEmitter };

  // Track whether we're currently in a paste operation to prevent useInput from processing paste content
  const isPastingRef = useRef(false);

  // Use Ink's useInput for regular keys - it handles batching properly
  // Skip processing when we're in the middle of a bracketed paste
  useInput((input, key) => {
    if (isPastingRef.current) {
      return;
    }
    handlerRef.current(input, {
      ...key,
      home: key.home ?? false,
      end: key.end ?? false,
      paste: false,
    });
  }, { isActive });

  // Listen for special sequences on the internal event emitter
  useEffect(() => {
    // Debug: check if event emitter exists
    logger.debug(`[useKeypress] useEffect: isActive=${isActive} hasEventEmitter=${!!internal_eventEmitter}`);

    if (!isActive || !internal_eventEmitter) return;

    let pasteBuffer = '';
    let isPasting = false;

    const handleInput = (data: string) => {
      // Debug: log all raw input
      const hex = Buffer.from(data).toString('hex');
      logger.debug(`[useKeypress] Raw input: len=${data.length} hex=${hex}`);

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
          isPastingRef.current = false;
          
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
          isPastingRef.current = false;
          
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
        const ctrlCCount = (data.match(/\x03/g) || []).length;
        // Debug log
        logger.debug(`[useKeypress] Multiple Ctrl+C detected: ${ctrlCCount} in chunk of length ${data.length}`);
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
