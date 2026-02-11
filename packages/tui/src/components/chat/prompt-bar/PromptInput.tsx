import { Box } from 'ink';
import path from 'path';
import React, { useEffect, useRef, useState, useLayoutEffect, useCallback, useMemo } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useKeypress, type Key } from '../../../hooks/useKeypress.js';
import { Text } from '../../ui/text/Text.js';
import { PastedChip, shouldCollapsePaste } from './PastedChip.js';
import { FileChip } from './FileChip.js';
import { normalizeLineEndings, isPrintable } from '../../../utils/index.js';
import { inputMetrics } from '../../../utils/inputMetrics.js';
import { useCommandState, useCommandActions, useFileAttachmentState, useFileAttachmentActions } from '../../../stores/selectors.js';
import { useAppStore } from '../../../stores/app-store.js';
import {
  type Segment,
  segmentWidth,
  totalWidth,
  getVisibleText,
  locateCursor,
  normalizeSegments,
  deleteWordBackward,
  killToEnd,
  killToBeginning,
  moveWordForward,
  moveWordBackward,
  transposeChars,
} from '../../../utils/input-editing.js';
import { CommandHistory } from '../../../utils/command-history.js';

// Calculate the visual display width of a chip
const getChipDisplayWidth = (seg: Segment): number => {
  if (seg.type === 'file') {
    const fileName = path.basename(seg.filePath);
    // Format: " {fileName}  {lineCount} lines " with background
    return ` ${fileName}  ${seg.lineCount} lines `.length;
  }
  if (seg.type === 'paste') {
    // Format: " {lineCount} lines " or " {charCount} chars "
    const label = seg.lineCount > 1 
      ? `${seg.lineCount} lines` 
      : `${seg.charCount} chars`;
    return ` ${label} `.length;
  }
  return 0;
};

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
type FileSegment = { type: 'file'; filePath: string; content: string; lineCount: number };
type PasteSegment = { type: 'paste'; content: string; lineCount: number; charCount: number };

// Build content for submission - use @file: markers for later expansion
const buildContent = (segments: Segment[]): string => {
  const parts = segments.map((s) => {
    if (s.type === 'text') return s.value;
    if (s.type === 'file') return ` @file:${s.filePath} `;
    if (s.type === 'paste') return s.content;
    return '';
  });
  return parts.join('').replace(/  +/g, ' ').trim();
};

// Detect trigger patterns
const detectTrigger = (text: string, cursor: number, rules: TriggerRule[]): TriggerInfo | null => {
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
  const { activeTrigger, filePickerHasResults, commandInputValue } = useCommandState();
  const { setCommandInput, clearCommandInput } = useCommandActions();
  const { pendingFileAttachment } = useFileAttachmentState();
  const { consumePendingFileAttachment } = useFileAttachmentActions();
  const { width: terminalWidth } = useTerminalSize();

  const [segments, setSegments] = useState<Segment[]>([{ type: 'text', value: '' }]);
  const [cursor, setCursor] = useState(0);

  const { getColor } = useTheme();
  const prevTriggerRef = useRef<TriggerInfo | null>(null);

  const secondaryColor = useMemo(() => getColor('secondary'), [getColor]);

  // Sync from store
  useEffect(() => {
    const visibleText = getVisibleText(segments);
    const firstSeg = segments[0];
    if (commandInputValue !== visibleText && segments.length === 1 && firstSeg?.type === 'text') {
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
          const fileSegment: FileSegment = { type: 'file', filePath, content, lineCount: lines.length };
          
          // Use stored trigger position to find where @query starts
          const { segIdx, offset } = locateCursor(segments, triggerPosition);
          const { segIdx: endSegIdx, offset: endOffset } = locateCursor(segments, cursor);
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
              if (s === fileSegment || (s.type === 'file' && s.filePath === filePath)) {
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
    const changed = (trigger === null) !== (prev === null) ||
      (trigger && prev && (trigger.key !== prev.key || trigger.position !== prev.position));
    if (changed) {
      onTriggerDetected(trigger);
      prevTriggerRef.current = trigger;
    }
  }, [segments, cursor, triggerRules, onTriggerDetected]);

  useLayoutEffect(() => {
    inputMetrics.markRenderComplete();
  });

  const syncToStore = useCallback((segs: Segment[]) => {
    setCommandInput(getVisibleText(segs));
  }, [setCommandInput]);

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
      const newValue = seg.value.slice(0, offset) + text + seg.value.slice(offset);
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
          if (s === pasteSegment || (s.type === 'paste' && s.content === normalized)) {
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
        newSegs[segIdx - 1] = { type: 'text', value: prevSeg.value.slice(0, -1) };
        setSegments(normalizeSegments(newSegs));
        setCursor(cursor - 1);
        syncToStore(newSegs);
      } else if (prevSeg) {
        // Delete the chip
        const newSegs = [...segments.slice(0, segIdx - 1), ...segments.slice(segIdx)];
        setSegments(normalizeSegments(newSegs));
        setCursor(cursor - 1);
        syncToStore(newSegs);
      }
    } else if (seg && seg.type !== 'text' && offset === 1) {
      // Cursor right after a chip - delete the chip
      const newSegs = [...segments.slice(0, segIdx), ...segments.slice(segIdx + 1)];
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

  useKeypress((userInput: string, key: Key) => {
    if (isProcessing) return;

    if (key.paste) {
      handlePaste(userInput);
      return;
    }

    if (key.return) {
      // Block Enter if file picker menu is visible with results
      if (activeTrigger?.key === '@' && filePickerHasResults) return;
      // Block Enter if slash command menu is visible (input starts with / and no space)
      if (activeTrigger?.key === '/' && !commandInputValue.includes(' ')) return;
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
      // Navigate to previous command in history
      const command = CommandHistory.getInstance().navigate('up');
      if (command) {
        setSegments([{ type: 'text', value: command }]);
        setCursor(command.length);
      }
    } else if (key.downArrow) {
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
  });

  // Render an inverse-color block cursor (single character)
  const renderCursor = (char: string = ' ') => {
    if (isProcessing) return null;
    return <Text inverse>{char}</Text>;
  };

  // Render text with cursor at the specified position
  const renderTextWithCursor = (text: string, cursorPos: number) => {
    if (isProcessing) {
      return <Text>{text}</Text>;
    }
    
    const before = text.slice(0, cursorPos);
    const charAtCursor = text[cursorPos] ?? ' ';
    const after = text.slice(cursorPos + 1);
    
    return (
      <>
        {before && <Text>{before}</Text>}
        {renderCursor(charAtCursor)}
        {after && <Text>{after}</Text>}
      </>
    );
  };

  const renderContent = () => {
    const total = totalWidth(segments);
    if (total === 0) {
      // Empty input - show cursor followed by placeholder
      return (
        <>
          {renderCursor()}
          <Text>{secondaryColor(placeholder)}</Text>
        </>
      );
    }

    // Calculate total visual width to determine if we need multi-line rendering
    let totalVisualWidth = 0;
    for (const seg of segments) {
      if (seg.type === 'text') {
        // For text, count characters but handle existing newlines
        const textLines = seg.value.split('\n');
        for (let i = 0; i < textLines.length; i++) {
          totalVisualWidth += textLines[i]!.length;
          if (i < textLines.length - 1) {
            // Reset for new line
            totalVisualWidth = textLines[i + 1]!.length;
          }
        }
      } else {
        totalVisualWidth += getChipDisplayWidth(seg);
      }
    }

    // Check if any text segment has newlines OR if content would wrap
    const hasExplicitNewlines = segments.some(s => s.type === 'text' && s.value.includes('\n'));
    const wouldWrap = totalVisualWidth > terminalWidth - 2; // -2 for some margin
    
    if (hasExplicitNewlines || wouldWrap) {
      // Multi-line rendering with pre-wrapping
      const lines: React.ReactNode[] = [];
      let currentLine: React.ReactNode[] = [];
      let globalPos = 0;
      let currentLineWidth = 0;
      const maxLineWidth = terminalWidth - 2; // Leave some margin
      
      for (let i = 0; i < segments.length; i++) {
        const seg = segments[i]!;
        
        if (seg.type === 'text') {
          const textLines = seg.value.split('\n');
          for (let li = 0; li < textLines.length; li++) {
            const lineText = textLines[li]!;
            
            // Check if this text needs to be wrapped
            if (currentLineWidth + lineText.length > maxLineWidth && currentLineWidth > 0) {
              // Start a new line before adding this text
              lines.push(<Box key={lines.length}>{currentLine}</Box>);
              currentLine = [];
              currentLineWidth = 0;
            }
            
            // If the text itself is longer than max width, we need to split it
            let remainingText = lineText;
            let textStartPos = globalPos;
            
            while (remainingText.length > 0) {
              const availableWidth = maxLineWidth - currentLineWidth;
              const chunkLength = Math.min(remainingText.length, availableWidth > 0 ? availableWidth : maxLineWidth);
              const chunk = remainingText.slice(0, chunkLength);
              remainingText = remainingText.slice(chunkLength);
              
              const chunkStart = textStartPos;
              const chunkEnd = textStartPos + chunk.length;
              const cursorInChunk = cursor >= chunkStart && cursor <= chunkEnd;
              
              if (cursorInChunk && !isProcessing) {
                const localCursor = cursor - chunkStart;
                currentLine.push(
                  <React.Fragment key={`${i}-${li}-${textStartPos}`}>
                    {renderTextWithCursor(chunk, localCursor)}
                  </React.Fragment>
                );
              } else {
                currentLine.push(<Text key={`${i}-${li}-${textStartPos}`}>{chunk}</Text>);
              }
              
              currentLineWidth += chunk.length;
              textStartPos += chunk.length;
              
              // If there's more text and we've filled the line, wrap
              if (remainingText.length > 0) {
                lines.push(<Box key={lines.length}>{currentLine}</Box>);
                currentLine = [];
                currentLineWidth = 0;
              }
            }
            
            globalPos += lineText.length;
            
            // Handle explicit newline (except for last part)
            if (li < textLines.length - 1) {
              lines.push(<Box key={lines.length}>{currentLine}</Box>);
              currentLine = [];
              currentLineWidth = 0;
              globalPos += 1; // for the \n
            }
          }
        } else {
          // Chip segment
          const chipWidth = getChipDisplayWidth(seg);
          
          // Check if chip would overflow current line
          if (currentLineWidth + chipWidth > maxLineWidth && currentLineWidth > 0) {
            lines.push(<Box key={lines.length}>{currentLine}</Box>);
            currentLine = [];
            currentLineWidth = 0;
          }
          
          const cursorInSeg = cursor >= globalPos && cursor <= globalPos + 1;
          if (seg.type === 'file') {
            if (cursorInSeg && cursor === globalPos && !isProcessing) {
              currentLine.push(<React.Fragment key={i}>{renderCursor()}<FileChip filePath={seg.filePath} lineCount={seg.lineCount} /></React.Fragment>);
            } else {
              currentLine.push(<FileChip key={i} filePath={seg.filePath} lineCount={seg.lineCount} />);
            }
          } else if (seg.type === 'paste') {
            if (cursorInSeg && cursor === globalPos && !isProcessing) {
              currentLine.push(<React.Fragment key={i}>{renderCursor()}<PastedChip lineCount={seg.lineCount} charCount={seg.charCount} /></React.Fragment>);
            } else {
              currentLine.push(<PastedChip key={i} lineCount={seg.lineCount} charCount={seg.charCount} />);
            }
          }
          currentLineWidth += chipWidth;
          globalPos += 1;
        }
      }
      
      if (currentLine.length > 0) {
        lines.push(<Box key={lines.length}>{currentLine}</Box>);
      }
      
      return <Box flexDirection="column">{lines}</Box>;
    }

    // Single-line rendering (content fits on one line)
    const elements: React.ReactNode[] = [];
    let pos = 0;

    for (let i = 0; i < segments.length; i++) {
      const seg = segments[i]!;
      const w = segmentWidth(seg);
      const cursorInSeg = cursor >= pos && cursor <= pos + w;

      if (seg.type === 'text') {
        if (cursorInSeg && !isProcessing) {
          const localCursor = cursor - pos;
          elements.push(
            <React.Fragment key={i}>
              {renderTextWithCursor(seg.value, localCursor)}
            </React.Fragment>
          );
        } else {
          elements.push(<Text key={i}>{seg.value}</Text>);
        }
      } else if (seg.type === 'file') {
        if (cursorInSeg && cursor === pos && !isProcessing) {
          elements.push(
            <React.Fragment key={i}>
              {renderCursor()}
              <FileChip filePath={seg.filePath} lineCount={seg.lineCount} />
            </React.Fragment>
          );
        } else if (cursorInSeg && cursor === pos + 1 && !isProcessing) {
          elements.push(
            <React.Fragment key={i}>
              <FileChip filePath={seg.filePath} lineCount={seg.lineCount} />
              {i === segments.length - 1 && renderCursor()}
            </React.Fragment>
          );
        } else {
          elements.push(<FileChip key={i} filePath={seg.filePath} lineCount={seg.lineCount} />);
        }
      } else if (seg.type === 'paste') {
        if (cursorInSeg && cursor === pos && !isProcessing) {
          elements.push(
            <React.Fragment key={i}>
              {renderCursor()}
              <PastedChip lineCount={seg.lineCount} charCount={seg.charCount} />
            </React.Fragment>
          );
        } else if (cursorInSeg && cursor === pos + 1 && !isProcessing) {
          elements.push(
            <React.Fragment key={i}>
              <PastedChip lineCount={seg.lineCount} charCount={seg.charCount} />
              {i === segments.length - 1 && renderCursor()}
            </React.Fragment>
          );
        } else {
          elements.push(<PastedChip key={i} lineCount={seg.lineCount} charCount={seg.charCount} />);
        }
      }
      pos += w;
    }

    return <>{elements}</>;
  };

  return <Box>{renderContent()}</Box>;
});
