import { Box } from 'ink';
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
import { PastedChip, shouldCollapsePaste } from './PastedChip.js';
import { FileChip } from './FileChip.js';
import { normalizeLineEndings, isPrintable } from '../../../utils/index.js';
import { logger } from '../../../utils/logger.js';
import { inputMetrics } from '../../../utils/inputMetrics.js';
import {
  useCommandState,
  useCommandActions,
  useFileAttachmentState,
  useFileAttachmentActions,
  useKiroClient,
  useImageAttachmentActions,
  useImageAttachmentState,
} from '../../../stores/selectors.js';
import {
  type Segment,
  segmentWidth,
  totalWidth,
  getVisibleText,
  locateCursor,
  normalizeSegments,
  deleteWordBackward,
  deleteForward,
  killToEnd,
  killToBeginning,
  moveWordForward,
  moveWordBackward,
  transposeChars,
} from '../../../utils/input-editing.js';
import { CommandHistory } from '../../../utils/command-history.js';

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
      return { key: rule.key, position: 0, type: rule.type };
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
  isProcessing,
  triggerRules = [],
  onTriggerDetected,
  placeholder = 'ask a question, or describe a task ↵',
}: PromptInputProps) {
  const { activeTrigger, filePickerHasResults, commandInputValue } =
    useCommandState();
  const { setCommandInput, clearCommandInput } = useCommandActions();
  const { pendingFileAttachment } = useFileAttachmentState();
  const { consumePendingFileAttachment } = useFileAttachmentActions();
  const { kiro } = useKiroClient();
  const { pendingImages } = useImageAttachmentState();
  const { addPendingImage } = useImageAttachmentActions();
  const [segments, setSegments] = useState<Segment[]>([
    { type: 'text', value: '' },
  ]);
  const [cursor, setCursor] = useState(0);

  const { getColor } = useTheme();
  const prevTriggerRef = useRef<TriggerInfo | null>(null);

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

  const syncToStore = useCallback(
    (segs: Segment[]) => {
      setCommandInput(getVisibleText(segs));
    },
    [setCommandInput]
  );

  const clearAll = () => {
    setSegments([{ type: 'text', value: '' }]);
    setCursor(0);
    clearCommandInput();
  };

  const insertText = (text: string) => {
    inputMetrics.markStateUpdate();
    const { segIdx, offset } = locateCursor(segments, cursor);
    const seg = segments[segIdx];

    if (seg?.type === 'text') {
      const newValue =
        seg.value.slice(0, offset) + text + seg.value.slice(offset);
      const newSegs = [...segments];
      newSegs[segIdx] = { type: 'text', value: newValue };
      setSegments(newSegs);
      setCursor(cursor + text.length);
      syncToStore(newSegs);
    } else if (seg) {
      // On a chip - insert text after it
      const newSegs = [
        ...segments.slice(0, segIdx + 1),
        { type: 'text' as const, value: text },
        ...segments.slice(segIdx + 1),
      ];
      setSegments(normalizeSegments(newSegs));
      setCursor(cursor + text.length);
      syncToStore(newSegs);
    }
  };

  const handlePaste = (pastedText: string) => {
    const normalized = normalizeLineEndings(pastedText);
    const result = shouldCollapsePaste(normalized);

    if (result.shouldCollapse) {
      const pasteSegment: PasteSegment = {
        type: 'paste',
        content: normalized,
        lineCount: result.lineCount,
        charCount: normalized.length,
      };
      const { segIdx, offset } = locateCursor(segments, cursor);
      const seg = segments[segIdx];

      if (seg?.type === 'text') {
        const newSegs = normalizeSegments([
          ...segments.slice(0, segIdx),
          { type: 'text', value: seg.value.slice(0, offset) },
          pasteSegment,
          { type: 'text', value: seg.value.slice(offset) },
          ...segments.slice(segIdx + 1),
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
        command: 'pasteImage',
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
      const { segIdx, offset } = locateCursor(segments, cursor);
      const seg = segments[segIdx];
      if (seg?.type === 'text') {
        const newSegs = normalizeSegments([
          ...segments.slice(0, segIdx),
          { type: 'text', value: seg.value.slice(0, offset) },
          imageSegment,
          { type: 'text', value: seg.value.slice(offset) },
          ...segments.slice(segIdx + 1),
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
    inputMetrics.markStateUpdate();
    if (cursor === 0) return;

    const { segIdx, offset } = locateCursor(segments, cursor);
    const seg = segments[segIdx];

    if (seg?.type === 'text' && offset > 0) {
      // Delete char in text
      const newValue = seg.value.slice(0, offset - 1) + seg.value.slice(offset);
      const newSegs = [...segments];
      newSegs[segIdx] = { type: 'text', value: newValue };
      setSegments(normalizeSegments(newSegs));
      setCursor(cursor - 1);
      syncToStore(newSegs);
    } else if (offset === 0 && segIdx > 0) {
      // At start of segment - delete previous segment/char
      const prevSeg = segments[segIdx - 1];
      if (prevSeg?.type === 'text') {
        // Delete last char of previous text
        const newSegs = [...segments];
        newSegs[segIdx - 1] = {
          type: 'text',
          value: prevSeg.value.slice(0, -1),
        };
        setSegments(normalizeSegments(newSegs));
        setCursor(cursor - 1);
        syncToStore(newSegs);
      } else if (prevSeg) {
        // Delete the chip
        const newSegs = [
          ...segments.slice(0, segIdx - 1),
          ...segments.slice(segIdx),
        ];
        setSegments(normalizeSegments(newSegs));
        setCursor(cursor - 1);
        syncToStore(newSegs);
      }
    } else if (seg && seg.type !== 'text' && offset === 1) {
      // Cursor right after a chip - delete the chip
      const newSegs = [
        ...segments.slice(0, segIdx),
        ...segments.slice(segIdx + 1),
      ];
      setSegments(normalizeSegments(newSegs));
      setCursor(cursor - 1);
      syncToStore(newSegs);
    }
  };

  // Helper to apply an edit result from utility functions
  const applyEdit = (result: { segments: Segment[]; cursor: number }) => {
    inputMetrics.markStateUpdate();
    setSegments(result.segments);
    setCursor(result.cursor);
    syncToStore(result.segments);
  };

  useKeypress(
    (userInput: string, key: Key) => {
      if (key.paste) {
        handlePaste(userInput);
        return;
      }

      // Check if slash command menu is visible
      const slashMenuVisible =
        activeTrigger?.key === '/' && !commandInputValue.includes(' ');
      // Check if file picker menu is visible
      const filePickerVisible =
        activeTrigger?.key === '@' && filePickerHasResults;

      if (key.return) {
        // Block Enter if file picker menu is visible with results
        if (filePickerVisible) return;
        // Block Enter if slash command menu is visible
        if (slashMenuVisible) return;
        const content = buildContent(segments);
        if (content) {
          clearAll();
          onSubmit(content);
        }
      } else if (key.backspace || key.delete) {
        handleBackspace();
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
        // Skip if menu is visible - let menu handle it
        if (slashMenuVisible || filePickerVisible) return;
        // Navigate to previous command in history
        const command = CommandHistory.getInstance().navigate('up');
        if (command) {
          setSegments([{ type: 'text', value: command }]);
          setCursor(command.length);
        }
      } else if (key.downArrow) {
        // Skip if menu is visible - let menu handle it
        if (slashMenuVisible || filePickerVisible) return;
        // Navigate to next command in history
        const command = CommandHistory.getInstance().navigate('down');
        if (command) {
          setSegments([{ type: 'text', value: command }]);
          setCursor(command.length);
        } else {
          // Returned to current input
          setSegments([{ type: 'text', value: '' }]);
          setCursor(0);
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
            setCursor(0);
            break;
          case 'e': // Ctrl+E - end of line
            inputMetrics.markStateUpdate();
            setCursor(totalWidth(segments));
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
            applyEdit(killToEnd(segments, cursor));
            break;
          case 'u': // Ctrl+U - kill to beginning of line
            applyEdit(killToBeginning(segments, cursor));
            break;
          case 't': // Ctrl+T - transpose characters
            applyEdit(transposeChars(segments, cursor));
            break;
          case 'j': // Ctrl+J - newline (existing)
            insertText('\n');
            break;
          case 'v': // Ctrl+V - paste image from clipboard
            handlePasteImage();
            break;
          default:
            break;
        }
      } else if (key.meta) {
        // Alt/Meta shortcuts (word movement)
        switch (userInput) {
          case 'b': // Alt+B - back one word
            inputMetrics.markStateUpdate();
            setCursor(moveWordBackward(segments, cursor));
            break;
          case 'f': // Alt+F - forward one word
            inputMetrics.markStateUpdate();
            setCursor(moveWordForward(segments, cursor));
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
    const total = totalWidth(segments);
    if (total === 0) {
      return (
        <>
          <Text inverse> </Text>
          <Text>{placeholderColor(placeholder)}</Text>
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
          const charAtCursor = seg.value[localCursor] ?? ' ';
          const after =
            localCursor < seg.value.length
              ? seg.value.slice(localCursor + 1)
              : '';
          parts.push(
            <React.Fragment key={i}>
              <Text>
                {styleInputText(seg.value.slice(0, localCursor), i === 0)}
              </Text>
              <Text inverse>{charAtCursor}</Text>
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
              <Text inverse> </Text>
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
              <Text inverse> </Text>
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
              <Text inverse> </Text>
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
          <Text key="cursor-end" inverse>
            {' '}
          </Text>
        );
      }
    }

    return <Text wrap="wrap">{parts}</Text>;
  };

  return <Box>{renderContent()}</Box>;
});
