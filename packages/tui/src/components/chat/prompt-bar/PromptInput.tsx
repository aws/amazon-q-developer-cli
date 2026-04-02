import { Box, CURSOR_MARKER } from './../../../renderer.js';
import React, {
  useEffect,
  useRef,
  useState,
  useLayoutEffect,
  useCallback,
  useMemo,
} from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeypress, type Key } from '../../../hooks/useKeypress.js';
import { Text } from '../../ui/text/Text.js';

/** Visual cursor (inverse block) + hardware cursor marker (APC sequence for twinki IME positioning). */
const CursorBlock = ({ char = ' ' }: { char?: string }) => (
  <>
    <Text>{CURSOR_MARKER}</Text>
    <Text inverse>{char}</Text>
  </>
);
import { PastedChip, shouldCollapsePaste } from './PastedChip.js';
import { FileChip } from './FileChip.js';
import {
  normalizeLineEndings,
  isPrintable,
  unescapeShellPath,
} from '../../../utils/index.js';
import { completePathAtCursor } from '../../../utils/path-completion.js';
import { logger } from '../../../utils/logger.js';
import { inputMetrics } from '../../../utils/inputMetrics.js';
import {
  useCommandState,
  useCommandActions,
  useFileAttachmentState,
  useFileAttachmentActions,
  useKiroClient,
  useImageAttachmentActions,
} from '../../../stores/selectors.js';
import {
  type Segment,
  segmentWidth,
  totalWidth,
  getVisibleText,
  locateCursor,
  normalizeSegments,
  deleteWordBackward,
  deleteWordForward,
  deleteForward,
  moveWordForward,
  moveWordBackward,
  transposeChars,
  uppercaseWord,
  lowercaseWord,
  capitalizeWord,
  transposeWords,
  isVisuallyMultiLine,
  moveCursorUpVisual,
  moveCursorDownVisual,
  moveToLogicalLineStart,
  moveToLogicalLineEnd,
  killToLogicalLineEnd,
  killToLogicalLineBeginning,
} from '../../../utils/input-editing.js';
import { CommandHistory } from '../../../utils/command-history.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';

export interface TriggerRule {
  key: string;
  type: 'start' | 'inline';
}

export interface TriggerInfo {
  key: string;
  position: number;
  type: 'start' | 'inline';
}

export interface PromptInputProps {
  onSubmit: (command: string) => void;
  isProcessing: boolean;
  triggerRules?: TriggerRule[];
  onTriggerDetected?: (trigger: TriggerInfo | null) => void;
  placeholder?: string;
}

// FileSegment type for local use
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
  base64: string;
  mimeType: string;
  width: number;
  height: number;
  sizeBytes: number;
};

// Build content for submission - use @file: markers for later expansion
const buildContent = (segments: Segment[]): string => {
  const parts = segments.map((s) => {
    if (s.type === 'text') return s.value;
    if (s.type === 'file') return ` @file:${s.filePath} `;
    if (s.type === 'paste') return s.content;
    // Images are handled separately via extractImages
    return '';
  });
  return parts.join('').replace(/  +/g, ' ').trim();
};

// Detect trigger patterns
const detectTrigger = (
  text: string,
  cursor: number,
  rules: TriggerRule[]
): TriggerInfo | null => {
  for (const rule of rules) {
    if (rule.type === 'start' && text.startsWith(rule.key)) {
      // Only trigger when cursor is within the command name (before first space)
      const spaceIndex = text.indexOf(' ');
      if (spaceIndex === -1 || cursor <= spaceIndex) {
        return { key: rule.key, position: 0, type: rule.type };
      }
    }
    if (rule.type === 'inline') {
      const lastIndex = text.slice(0, cursor).lastIndexOf(rule.key);
      if (lastIndex !== -1) {
        return { key: rule.key, position: lastIndex, type: rule.type };
      }
    }
  }
  return null;
};

export const PromptInput = React.memo(function PromptInput({
  onSubmit,
  triggerRules = [],
  onTriggerDetected,
  placeholder = 'ask a question, or describe a task ↵',
}: PromptInputProps) {
  const {
    activeTrigger,
    filePickerHasResults,
    commandInputValue,
    promptHint,
    slashCommands,
    activeCommand,
  } = useCommandState();
  const { setCommandInput, clearCommandInput, setPromptHint } =
    useCommandActions();
  const { pendingFileAttachment } = useFileAttachmentState();
  const { consumePendingFileAttachment } = useFileAttachmentActions();
  const { kiro } = useKiroClient();
  const { addPendingImage } = useImageAttachmentActions();
  const [segments, _setSegments] = useState<Segment[]>([
    { type: 'text', value: '' },
  ]);
  const [cursor, _setCursor] = useState(0);
  const [pathCandidates, setPathCandidates] = useState<string[]>([]);

  // Refs shadow the latest state so input handlers never read stale closures.
  // Without these, keypresses arriving faster than React re-renders would
  // read the old segments/cursor and overwrite each other's edits.
  const segmentsRef = useRef(segments);
  const cursorRef = useRef(cursor);
  const setSegments = useCallback(
    (s: Segment[] | ((prev: Segment[]) => Segment[])) => {
      if (typeof s === 'function') {
        _setSegments((prev) => {
          const next = s(prev);
          segmentsRef.current = next;
          return next;
        });
      } else {
        segmentsRef.current = s;
        _setSegments(s);
      }
    },
    []
  );
  const setCursor = useCallback((c: number | ((prev: number) => number)) => {
    if (typeof c === 'function') {
      _setCursor((prev) => {
        const next = c(prev);
        cursorRef.current = next;
        return next;
      });
    } else {
      cursorRef.current = c;
      _setCursor(c);
    }
  }, []);

  const undoStack = useRef<Array<{ segments: Segment[]; cursor: number }>>([]);
  const lastUndoPushTime = useRef(0);

  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const prevTriggerRef = useRef<TriggerInfo | null>(null);
  const suppressNextTriggerRef = useRef(false);

  const primaryColor = useMemo(() => getColor('primary'), [getColor]);
  const brandColor = useMemo(() => getColor('brand'), [getColor]);
  const styleInputText = useCallback(
    (text: string, isFirstSegment: boolean) => {
      const fullText = getVisibleText(segments);
      if (fullText.startsWith('!') && isFirstSegment && text.length > 0) {
        if (text.startsWith('!')) {
          return brandColor('!') + primaryColor(text.slice(1));
        }
      }
      return primaryColor(text);
    },
    [segments, brandColor, primaryColor]
  );
  const placeholderColor = useMemo(() => getColor('muted'), [getColor]);

  useEffect(() => {
    if (localSyncRef.current) {
      localSyncRef.current = false;
      return;
    }
    const visibleText = getVisibleText(segments);
    const firstSeg = segments[0];
    if (
      commandInputValue !== visibleText &&
      segments.length === 1 &&
      firstSeg?.type === 'text'
    ) {
      setSegments([{ type: 'text', value: commandInputValue }]);
      setCursor(commandInputValue.length);
    }
  }, [commandInputValue]);

  // Consume pending file attachment
  useEffect(() => {
    if (pendingFileAttachment) {
      const pending = consumePendingFileAttachment();
      if (pending) {
        const { path: filePath, triggerPosition } = pending;
        const fs = require('fs');
        try {
          const content = fs.readFileSync(filePath, 'utf-8');
          const lines = content.split(/\r\n|\r|\n/);
          const fileSegment: FileSegment = {
            type: 'file',
            filePath,
            content,
            lineCount: lines.length,
          };

          // Use stored trigger position to find where @query starts
          const { segIdx, offset } = locateCursor(segments, triggerPosition);
          const { segIdx: endSegIdx, offset: endOffset } = locateCursor(
            segments,
            cursor
          );
          const seg = segments[segIdx];

          if (seg?.type === 'text' && segIdx === endSegIdx) {
            // Replace @query with file chip
            const newSegs = [
              ...segments.slice(0, segIdx),
              { type: 'text' as const, value: seg.value.slice(0, offset) },
              fileSegment,
              { type: 'text' as const, value: seg.value.slice(endOffset) },
              ...segments.slice(segIdx + 1),
            ];
            const normalized = normalizeSegments(newSegs);
            setSegments(normalized);
            // Position cursor after the chip
            let newCursor = 0;
            for (const s of normalized) {
              if (
                s === fileSegment ||
                (s.type === 'file' && s.filePath === filePath)
              ) {
                newCursor += 1;
                break;
              }
              newCursor += segmentWidth(s);
            }
            setCursor(newCursor);
            syncToStore(normalized);
          } else {
            // Fallback: insert at trigger position
            if (seg?.type === 'text') {
              const newSegs = [
                ...segments.slice(0, segIdx),
                { type: 'text' as const, value: seg.value.slice(0, offset) },
                fileSegment,
                { type: 'text' as const, value: seg.value.slice(offset) },
                ...segments.slice(segIdx + 1),
              ];
              const normalized = normalizeSegments(newSegs);
              setSegments(normalized);
              setCursor(triggerPosition + 1);
              syncToStore(normalized);
            }
          }
        } catch {
          // ignore
        }
      }
    }
  }, [pendingFileAttachment]);

  // Trigger detection
  useEffect(() => {
    if (!onTriggerDetected) return;
    // Skip trigger detection when content was restored from history navigation
    if (suppressNextTriggerRef.current) {
      suppressNextTriggerRef.current = false;
      onTriggerDetected(null);
      prevTriggerRef.current = null;
      return;
    }
    const text = getVisibleText(segments);
    const trigger = detectTrigger(text, cursor, triggerRules);
    const prev = prevTriggerRef.current;
    const changed =
      (trigger === null) !== (prev === null) ||
      (trigger &&
        prev &&
        (trigger.key !== prev.key || trigger.position !== prev.position));
    if (changed) {
      onTriggerDetected(trigger);
      prevTriggerRef.current = trigger;
    }
  }, [segments, cursor, triggerRules, onTriggerDetected]);

  useLayoutEffect(() => {
    inputMetrics.markRenderComplete();
  });

  const localSyncRef = useRef(false);

  const syncToStore = useCallback(
    (segs: Segment[]) => {
      const text = getVisibleText(segs);
      localSyncRef.current = true;
      setCommandInput(text);

      // Clear promptHint when user starts typing args (space after command)
      if (promptHint && text.startsWith('/') && text.includes(' ')) {
        setPromptHint(null);
      }
    },
    [setCommandInput, setPromptHint, promptHint]
  );

  const clearAll = () => {
    setSegments([{ type: 'text', value: '' }]);
    setCursor(0);
    clearCommandInput();
  };

  const insertText = (text: string) => {
    pushUndo();
    inputMetrics.markStateUpdate();
    const segs = segmentsRef.current;
    const cur = cursorRef.current;
    const { segIdx, offset } = locateCursor(segs, cur);
    const seg = segs[segIdx];

    if (seg?.type === 'text') {
      const newValue =
        seg.value.slice(0, offset) + text + seg.value.slice(offset);
      const newSegs = [...segs];
      newSegs[segIdx] = { type: 'text', value: newValue };
      setSegments(newSegs);
      setCursor(cur + text.length);
      syncToStore(newSegs);
    } else if (seg) {
      // On a chip - insert text after it
      const newSegs = [
        ...segs.slice(0, segIdx + 1),
        { type: 'text' as const, value: text },
        ...segs.slice(segIdx + 1),
      ];
      setSegments(normalizeSegments(newSegs));
      setCursor(cur + text.length);
      syncToStore(newSegs);
    }
  };

  const handlePaste = (pastedText: string) => {
    // Unescape shell-escaped file paths from drag-and-drop (e.g. macOS Finder)
    const unescaped = unescapeShellPath(pastedText);
    const normalized = normalizeLineEndings(unescaped);
    const result = shouldCollapsePaste(normalized);

    if (result.shouldCollapse) {
      pushUndo();
      const pasteSegment: PasteSegment = {
        type: 'paste',
        content: normalized,
        lineCount: result.lineCount,
        charCount: normalized.length,
      };
      const segs = segmentsRef.current;
      const cur = cursorRef.current;
      const { segIdx, offset } = locateCursor(segs, cur);
      const seg = segs[segIdx];

      if (seg?.type === 'text') {
        const newSegs = normalizeSegments([
          ...segs.slice(0, segIdx),
          { type: 'text', value: seg.value.slice(0, offset) },
          pasteSegment,
          { type: 'text', value: seg.value.slice(offset) },
          ...segs.slice(segIdx + 1),
        ]);
        setSegments(newSegs);
        // Position cursor after the chip
        let newCursor = 0;
        for (let i = 0; i < newSegs.length; i++) {
          const s = newSegs[i]!;
          if (
            s === pasteSegment ||
            (s.type === 'paste' && s.content === normalized)
          ) {
            newCursor += 1;
            break;
          }
          newCursor += segmentWidth(s);
        }
        setCursor(newCursor);
      }
      return;
    }

    insertText(normalized);
  };

  const handlePasteImage = async () => {
    try {
      const result = await kiro.executeCommand({
        command: 'paste',
        args: {},
      });
      if (!result.success) {
        // No image in clipboard or error — fall back to normal paste
        return false;
      }
      const data = result.data as {
        data: string;
        mimeType: string;
        width: number;
        height: number;
        sizeBytes: number;
      };
      const imageSegment: ImageSegment = {
        type: 'image',
        base64: data.data,
        mimeType: data.mimeType,
        width: data.width,
        height: data.height,
        sizeBytes: data.sizeBytes,
      };
      // Insert image segment at cursor
      const { segIdx, offset } = locateCursor(
        segmentsRef.current,
        cursorRef.current
      );
      const seg = segmentsRef.current[segIdx];
      if (seg?.type === 'text') {
        const newSegs = normalizeSegments([
          ...segmentsRef.current.slice(0, segIdx),
          { type: 'text', value: seg.value.slice(0, offset) },
          imageSegment,
          { type: 'text', value: seg.value.slice(offset) },
          ...segmentsRef.current.slice(segIdx + 1),
        ]);
        setSegments(newSegs);
        // Position cursor after the chip
        let newCursor = 0;
        for (const s of newSegs) {
          newCursor += segmentWidth(s);
          if (
            s === imageSegment ||
            (s.type === 'image' && s.base64 === data.data)
          ) {
            break;
          }
        }
        setCursor(newCursor);
      }
      // Also add to store so sendMessage includes it
      addPendingImage({
        base64: data.data,
        mimeType: data.mimeType,
        width: data.width,
        height: data.height,
        sizeBytes: data.sizeBytes,
      });
      return true;
    } catch (e) {
      logger.error('[PromptInput] handlePasteImage error:', e);
      return false;
    }
  };

  const handleBackspace = () => {
    pushUndo();
    inputMetrics.markStateUpdate();
    const cur = cursorRef.current;
    const segs = segmentsRef.current;
    if (cur === 0) return;

    const { segIdx, offset } = locateCursor(segs, cur);
    const seg = segs[segIdx];

    if (seg?.type === 'text' && offset > 0) {
      // Delete char in text
      const newValue = seg.value.slice(0, offset - 1) + seg.value.slice(offset);
      const newSegs = [...segs];
      newSegs[segIdx] = { type: 'text', value: newValue };
      setSegments(normalizeSegments(newSegs));
      setCursor(cur - 1);
      syncToStore(newSegs);
    } else if (offset === 0 && segIdx > 0) {
      // At start of segment - delete previous segment/char
      const prevSeg = segs[segIdx - 1];
      if (prevSeg?.type === 'text') {
        // Delete last char of previous text
        const newSegs = [...segs];
        newSegs[segIdx - 1] = {
          type: 'text',
          value: prevSeg.value.slice(0, -1),
        };
        setSegments(normalizeSegments(newSegs));
        setCursor(cur - 1);
        syncToStore(newSegs);
      } else if (prevSeg) {
        // Delete the chip
        const newSegs = [...segs.slice(0, segIdx - 1), ...segs.slice(segIdx)];
        setSegments(normalizeSegments(newSegs));
        setCursor(cur - 1);
        syncToStore(newSegs);
      }
    } else if (seg && seg.type !== 'text' && offset === 1) {
      // Cursor right after a chip - delete the chip
      const newSegs = [...segs.slice(0, segIdx), ...segs.slice(segIdx + 1)];
      setSegments(normalizeSegments(newSegs));
      setCursor(cur - 1);
      syncToStore(newSegs);
    }
  };

  const pushUndo = (force = false) => {
    const now = Date.now();
    if (!force && now - lastUndoPushTime.current < 500) return;
    lastUndoPushTime.current = now;
    undoStack.current.push({
      segments: segmentsRef.current,
      cursor: cursorRef.current,
    });
    if (undoStack.current.length > 100) undoStack.current.shift();
  };

  // Helper to apply an edit result from utility functions
  const applyEdit = (result: { segments: Segment[]; cursor: number }) => {
    pushUndo(true);
    inputMetrics.markStateUpdate();
    setSegments(result.segments);
    setCursor(result.cursor);
    syncToStore(result.segments);
  };

  useKeypress(
    (userInput: string, key: Key) => {
      // Read latest state from refs to avoid stale closures when keypresses
      // arrive faster than React can re-render.
      const segments = segmentsRef.current;
      const cursor = cursorRef.current;

      // Don't process input when selection menu is open (Menu handles its own input)
      if (activeCommand) return;

      if (key.paste) {
        handlePaste(userInput);
        return;
      }

      // Clear path completion candidates on any key except Tab
      if (!key.tab) {
        setPathCandidates([]);
      }

      // Check if slash command menu is visible (has matching commands)
      const hasMatchingSlashCommands =
        activeTrigger?.key === '/' && !commandInputValue.includes(' ')
          ? slashCommands.some((cmd) =>
              cmd.name
                .slice(1)
                .toLowerCase()
                .startsWith(commandInputValue.slice(1).toLowerCase())
            )
          : false;
      const slashMenuVisible = hasMatchingSlashCommands;
      // Check if file picker menu is visible
      const filePickerVisible =
        activeTrigger?.key === '@' && filePickerHasResults;

      if (key.return) {
        if (key.meta || key.shift) {
          // Alt+Enter or Shift+Enter - insert newline
          insertText('\n');
        } else {
          // Block Enter if file picker menu is visible with results
          if (filePickerVisible) return;
          // Block Enter if slash command menu is visible
          if (slashMenuVisible) return;
          const content = buildContent(segments);
          const hasImages = segments.some((s) => s.type === 'image');
          if (content || hasImages) {
            clearAll();
            onSubmit(content);
          }
        }
      } else if (key.tab && !key.shift) {
        // Tab completion for filesystem paths
        // Skip if a menu is handling tab
        if (slashMenuVisible || filePickerVisible) return;
        const text = getVisibleText(segments);
        const result = completePathAtCursor(text, cursor);
        if (result) {
          if (result.replacement !== text.slice(result.start, cursor)) {
            // Replace the token in segments with the completed path
            const before = text.slice(0, result.start);
            const after = text.slice(cursor);
            const newText = before + result.replacement + after;
            const newCursor = result.start + result.replacement.length;
            const newSegs: Segment[] = [{ type: 'text', value: newText }];
            setSegments(newSegs);
            setCursor(newCursor);
            syncToStore(newSegs);
          } else if (result.candidates.length > 1) {
            // No progress possible — show candidates list
            setPathCandidates(result.candidates);
          }
        }
      } else if (key.backspace || key.delete) {
        if (key.meta) {
          // Alt+Backspace / Alt+Delete - delete word backward (matches V1 rustyline behavior)
          applyEdit(deleteWordBackward(segments, cursor));
        } else {
          handleBackspace();
        }
      } else if (key.leftArrow) {
        inputMetrics.markStateUpdate();
        if (key.ctrl || key.meta) {
          // Ctrl+Left or Cmd+Left - move word backward
          setCursor(moveWordBackward(segments, cursor));
        } else {
          setCursor(Math.max(0, cursor - 1));
        }
      } else if (key.rightArrow) {
        inputMetrics.markStateUpdate();
        if (key.ctrl || key.meta) {
          // Ctrl+Right or Cmd+Right - move word forward
          setCursor(moveWordForward(segments, cursor));
        } else {
          setCursor(Math.min(totalWidth(segments), cursor + 1));
        }
      } else if (key.upArrow) {
        // shift+arrow is used by ActivityTray for navigation — don't handle here
        if (key.shift) return;
        // Skip if any menu is visible - let menu handle it
        if (slashMenuVisible || filePickerVisible || activeCommand) return;
        // Multi-line or visually wrapped: move cursor up a visual line
        if (isVisuallyMultiLine(segments, termWidth)) {
          const newPos = moveCursorUpVisual(segments, cursor, termWidth);
          if (newPos !== null) {
            inputMetrics.markStateUpdate();
            setCursor(newPos);
            return;
          }
        }
        // Single-line or already on first line: navigate history
        const currentText = buildContent(segments);
        const command = CommandHistory.getInstance().navigate(
          'up',
          currentText
        );
        if (command) {
          // Suppress trigger so slash commands from history don't open the menu
          suppressNextTriggerRef.current = true;
          setPromptHint(null);
          setSegments([{ type: 'text', value: command }]);
          setCursor(command.length);
        }
      } else if (key.downArrow) {
        // shift+arrow is used by ActivityTray for navigation — don't handle here
        if (key.shift) return;
        // Skip if any menu is visible - let menu handle it
        if (slashMenuVisible || filePickerVisible || activeCommand) return;
        // Multi-line or visually wrapped: move cursor down a visual line
        if (isVisuallyMultiLine(segments, termWidth)) {
          const newPos = moveCursorDownVisual(segments, cursor, termWidth);
          if (newPos !== null) {
            inputMetrics.markStateUpdate();
            setCursor(newPos);
            return;
          }
        }
        // Single-line or already on last line: navigate history
        // Skip if user is just editing (not browsing history) to avoid clearing input
        if (!CommandHistory.getInstance().isNavigating()) return;
        const command = CommandHistory.getInstance().navigate('down');
        if (command !== null) {
          suppressNextTriggerRef.current = true;
          setPromptHint(null);
          setSegments([{ type: 'text', value: command }]);
          setCursor(command.length);
        }
      } else if (key.home) {
        inputMetrics.markStateUpdate();
        setCursor(0);
      } else if (key.end) {
        inputMetrics.markStateUpdate();
        setCursor(totalWidth(segments));
      } else if (key.ctrl) {
        // Emacs/readline shortcuts
        switch (userInput) {
          case 'a': // Ctrl+A - beginning of line
            inputMetrics.markStateUpdate();
            setCursor(moveToLogicalLineStart(segments, cursor));
            break;
          case 'e': // Ctrl+E - end of line
            inputMetrics.markStateUpdate();
            setCursor(moveToLogicalLineEnd(segments, cursor));
            break;
          case 'b': // Ctrl+B - back one char
            inputMetrics.markStateUpdate();
            setCursor(Math.max(0, cursor - 1));
            break;
          case 'f': // Ctrl+F - forward one char
            inputMetrics.markStateUpdate();
            setCursor(Math.min(totalWidth(segments), cursor + 1));
            break;
          case 'd': // Ctrl+D - delete char under cursor (forward delete)
            applyEdit(deleteForward(segments, cursor));
            break;
          case 'w': // Ctrl+W - delete word backward
            applyEdit(deleteWordBackward(segments, cursor));
            break;
          case 'k': // Ctrl+K - kill to end of line
            applyEdit(killToLogicalLineEnd(segments, cursor));
            break;
          case 'u': // Ctrl+U - kill to beginning of line
            applyEdit(killToLogicalLineBeginning(segments, cursor));
            break;
          case 't': // Ctrl+T - transpose characters
            applyEdit(transposeChars(segments, cursor));
            break;
          case '_': // Ctrl+_ / Ctrl+/ — undo
            {
              const prev = undoStack.current.pop();
              if (prev) {
                inputMetrics.markStateUpdate();
                setSegments(prev.segments);
                setCursor(prev.cursor);
                syncToStore(prev.segments);
              }
            }
            break;
          case 'j': // Ctrl+J - newline (existing)
            insertText('\n');
            break;
          case 'p': // Ctrl+P - move cursor up / previous history
            {
              if (slashMenuVisible || filePickerVisible) break;
              if (isVisuallyMultiLine(segments, termWidth)) {
                const newPos = moveCursorUpVisual(segments, cursor, termWidth);
                if (newPos !== null) {
                  inputMetrics.markStateUpdate();
                  setCursor(newPos);
                  break;
                }
              }
              const command = CommandHistory.getInstance().navigate(
                'up',
                buildContent(segments)
              );
              if (command) {
                setPromptHint(null);
                setSegments([{ type: 'text', value: command }]);
                setCursor(command.length);
              }
            }
            break;
          case 'n': // Ctrl+N - move cursor down / next history
            {
              if (slashMenuVisible || filePickerVisible) break;
              if (isVisuallyMultiLine(segments, termWidth)) {
                const newPos = moveCursorDownVisual(
                  segments,
                  cursor,
                  termWidth
                );
                if (newPos !== null) {
                  inputMetrics.markStateUpdate();
                  setCursor(newPos);
                  break;
                }
              }
              // Skip if user is just editing (not browsing history) to avoid clearing input
              if (!CommandHistory.getInstance().isNavigating()) break;
              const command = CommandHistory.getInstance().navigate('down');
              if (command) {
                setPromptHint(null);
                setSegments([{ type: 'text', value: command }]);
                setCursor(command.length);
              } else {
                setPromptHint(null);
                setSegments([{ type: 'text', value: '' }]);
                setCursor(0);
              }
            }
            break;
          case 'l': // Ctrl+L - clear screen
            process.stdout.write('\x1b[2J\x1b[H');
            break;
          case 'v': // Ctrl+V - paste image from clipboard
            handlePasteImage();
            break;
          default:
            break;
        }
      } else if (key.meta) {
        // Alt/Meta shortcuts (word movement and deletion)
        switch (userInput) {
          case 'b': // Alt+B - back one word
            inputMetrics.markStateUpdate();
            setCursor(moveWordBackward(segments, cursor));
            break;
          case 'f': // Alt+F - forward one word
            inputMetrics.markStateUpdate();
            setCursor(moveWordForward(segments, cursor));
            break;
          case 'd': // Alt+D - delete word forward
            applyEdit(deleteWordForward(segments, cursor));
            break;
          case 't': // Alt+T - transpose words
            applyEdit(transposeWords(segments, cursor));
            break;
          case 'u': // Alt+U - uppercase word
            applyEdit(uppercaseWord(segments, cursor));
            break;
          case 'l': // Alt+L - lowercase word
            applyEdit(lowercaseWord(segments, cursor));
            break;
          case 'c': // Alt+C - capitalize word
            applyEdit(capitalizeWord(segments, cursor));
            break;
          default:
            break;
        }
      } else if (userInput && isPrintable(userInput)) {
        insertText(normalizeLineEndings(userInput));
      }
    },
    { onEmptyPaste: handlePasteImage }
  );

  const renderContent = () => {
    // When selection menu is open, show the command name statically (no cursor)
    if (activeCommand) {
      const cmdName = activeCommand.command.name;
      return <Text>{styleInputText(cmdName, true)}</Text>;
    }

    const total = totalWidth(segments);
    if (total === 0) {
      return (
        <>
          <CursorBlock />
          <Text>{placeholderColor(placeholder)}</Text>
          <Text>{placeholderColor('  ctrl+g: agent monitor')}</Text>
        </>
      );
    }

    // Build flat array of <Text> children. Ink's squashTextNodes flattens
    // nested <Text>/<ink-virtual-text> into one ANSI string, then wrap-ansi
    // wraps it at the container width using string-width (Unicode-correct).
    const parts: React.ReactNode[] = [];
    let pos = 0;

    for (let i = 0; i < segments.length; i++) {
      const seg = segments[i]!;
      const w = segmentWidth(seg);
      const cursorInSeg = cursor >= pos && cursor <= pos + w;

      if (seg.type === 'text') {
        if (cursorInSeg) {
          const localCursor = cursor - pos;
          // Handle the case that cursor is on the newline at the end of a line:
          // render a visible space for the cursor block, and keep the \n in `after`
          // so the line break still renders.
          const onNewline = seg.value[localCursor] === '\n';
          const charAtCursor = onNewline
            ? ' '
            : (seg.value[localCursor] ?? ' ');
          const afterStart = onNewline ? localCursor : localCursor + 1;
          const after =
            afterStart < seg.value.length ? seg.value.slice(afterStart) : '';
          parts.push(
            <React.Fragment key={i}>
              <Text>
                {styleInputText(seg.value.slice(0, localCursor), i === 0)}
              </Text>
              <CursorBlock char={charAtCursor} />
              {after && <Text>{primaryColor(after)}</Text>}
            </React.Fragment>
          );
        } else {
          parts.push(<Text key={i}>{styleInputText(seg.value, i === 0)}</Text>);
        }
      } else if (seg.type === 'file') {
        if (cursorInSeg && cursor === pos) {
          parts.push(
            // Text color handled by FileChip component (uses theme colors internally)
            <React.Fragment key={i}>
              <CursorBlock />
              <FileChip filePath={seg.filePath} lineCount={seg.lineCount} />
            </React.Fragment>
          );
        } else {
          parts.push(
            <FileChip
              key={i}
              filePath={seg.filePath}
              lineCount={seg.lineCount}
            />
          );
        }
      } else if (seg.type === 'paste') {
        if (cursorInSeg && cursor === pos) {
          parts.push(
            // Text color handled by PastedChip component (uses theme colors internally)
            <React.Fragment key={i}>
              <CursorBlock />
              <PastedChip lineCount={seg.lineCount} charCount={seg.charCount} />
            </React.Fragment>
          );
        } else {
          parts.push(
            <PastedChip
              key={i}
              lineCount={seg.lineCount}
              charCount={seg.charCount}
            />
          );
        }
      } else if (seg.type === 'image') {
        if (cursorInSeg && cursor === pos) {
          parts.push(
            <React.Fragment key={i}>
              <CursorBlock />
              <PastedChip
                type="image"
                imageWidth={seg.width}
                imageHeight={seg.height}
                imageSizeBytes={seg.sizeBytes}
              />
            </React.Fragment>
          );
        } else {
          parts.push(
            <PastedChip
              key={i}
              type="image"
              imageWidth={seg.width}
              imageHeight={seg.height}
              imageSizeBytes={seg.sizeBytes}
            />
          );
        }
      }
      pos += w;
    }

    // Trailing cursor after a chip at the end
    // inverse swaps fg/bg colors - no explicit color needed
    if (cursor === total) {
      const lastSeg = segments[segments.length - 1];
      if (lastSeg && lastSeg.type !== 'text') {
        parts.push(
          <React.Fragment key="cursor-end">
            <CursorBlock />
          </React.Fragment>
        );
      }
    }

    return <Text wrap="wrap">{parts}</Text>;
  };

  const candidateRows = useMemo(() => {
    if (pathCandidates.length === 0) return null;
    const sorted = [...pathCandidates].sort();
    const maxLen = Math.max(...sorted.map((c) => c.length));
    const colWidth = maxLen + 2;
    const cols = Math.max(1, Math.floor(termWidth / colWidth));
    const rows: string[][] = [];
    for (let i = 0; i < sorted.length; i += cols) {
      rows.push(sorted.slice(i, i + cols));
    }
    return { rows, colWidth };
  }, [pathCandidates, termWidth]);

  return (
    <Box flexDirection="column">
      <Box>{renderContent()}</Box>
      {candidateRows && (
        <Box flexDirection="column">
          {candidateRows.rows.map((row, ri) => (
            <Text key={ri} wrap="truncate">
              {row.map((c) => c.padEnd(candidateRows.colWidth)).join('')}
            </Text>
          ))}
        </Box>
      )}
    </Box>
  );
});
