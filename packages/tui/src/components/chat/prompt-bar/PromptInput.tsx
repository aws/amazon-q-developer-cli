import { Box } from 'ink';
import React, { useEffect, useRef, useState } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeypress, type Key } from '../../../hooks/useKeypress.js';
import { Text } from '../../ui/text/Text.js';
import { PastedChip, shouldCollapsePaste } from './PastedChip.js';
import { normalizeLineEndings, isPrintable } from '../../../utils/index.js';

export interface TriggerRule {
  key: string;
  type: 'start' | 'inline';
}

export interface TriggerInfo {
  key: string;
  position: number;
  type: 'start' | 'inline';
}

interface CollapsedContent {
  content: string;
  lineCount: number;
  charCount: number;
  prefixText: string;
}

export interface PromptInputProps {
  onSubmit: (command: string) => void;
  isProcessing: boolean;
  triggerRules?: TriggerRule[];
  onTriggerDetected?: (trigger: TriggerInfo | null) => void;
  onInputChange?: (value: string) => void;
  placeholder?: string;
  value?: string;
  clearOnSubmit?: boolean;
}

// Detect trigger patterns in text
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

// Build full content from collapsed + visible input
const buildFullContent = (
  collapsed: CollapsedContent | null,
  visibleInput: string
): string => {
  if (!collapsed) return visibleInput.trim();

  let result = collapsed.prefixText || '';
  if (result && !result.endsWith(' ') && !result.endsWith('\n')) {
    result += ' ';
  }
  result += collapsed.content;
  if (visibleInput) {
    if (!result.endsWith(' ') && !result.endsWith('\n')) {
      result += '\n';
    }
    result += visibleInput;
  }
  return result.trim();
};

type CursorRegion = 'prefix' | 'chip' | 'suffix';

export const PromptInput = React.memo(function PromptInput({
  onSubmit,
  isProcessing,
  triggerRules = [],
  onTriggerDetected,
  onInputChange,
  placeholder = 'ask a question, or describe a task ↵',
  value,
  clearOnSubmit = true,
}: PromptInputProps) {
  const [input, setInput] = useState(value ?? '');
  // Unified cursor position: prefixText chars + chip (1 char) + suffix chars
  const [cursor, setCursor] = useState(value?.length ?? 0);
  const [collapsed, setCollapsed] = useState<CollapsedContent | null>(null);

  const { getColor } = useTheme();
  const prevTriggerRef = useRef<TriggerInfo | null>(null);
  const lastValueRef = useRef(value ?? '');

  // Get total length treating chip as 1 char
  const getTotalLength = (): number => {
    if (!collapsed) return input.length;
    return collapsed.prefixText.length + 1 + input.length;
  };

  // Get cursor region and local position within that region
  const getCursorInfo = (): { region: CursorRegion; localPos: number } => {
    if (!collapsed) {
      return { region: 'suffix', localPos: cursor };
    }
    const prefixLen = collapsed.prefixText.length;
    if (cursor < prefixLen) {
      return { region: 'prefix', localPos: cursor };
    } else if (cursor === prefixLen) {
      // Cursor is right before the chip (at end of prefix)
      return { region: 'prefix', localPos: cursor };
    } else if (cursor === prefixLen + 1) {
      // Cursor is right after the chip
      return { region: 'suffix', localPos: 0 };
    } else {
      return { region: 'suffix', localPos: cursor - prefixLen - 1 };
    }
  };

  // Sync from controlled value (when not collapsed)
  useEffect(() => {
    if (value === undefined || collapsed || value === input) return;
    if (value === lastValueRef.current) return;

    setInput(value);
    setCursor(value.length);
    lastValueRef.current = value;
  }, [value, collapsed, input]);

  // Notify parent of trigger changes
  useEffect(() => {
    if (!onTriggerDetected) return;

    const trigger = detectTrigger(input, cursor, triggerRules);
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
  }, [input, cursor, triggerRules, onTriggerDetected]);

  const updateInput = (newInput: string, newCursor: number) => {
    setInput(newInput);
    setCursor(newCursor);
    lastValueRef.current = newInput;
    if (!collapsed) onInputChange?.(newInput);
  };

  const clearAll = () => {
    setCollapsed(null);
    setInput('');
    setCursor(0);
    lastValueRef.current = '';
    onInputChange?.('');
  };

  const handlePaste = (pastedText: string) => {
    const normalized = normalizeLineEndings(pastedText);
    const result = shouldCollapsePaste(normalized);

    if (result.shouldCollapse) {
      const { region, localPos } = getCursorInfo();
      let prefixText: string;
      let suffixText: string;

      if (collapsed) {
        if (region === 'prefix') {
          // Pasting in prefix: prefix before cursor + old content becomes new prefix
          prefixText = collapsed.prefixText.slice(0, localPos);
          suffixText = collapsed.prefixText.slice(localPos) + collapsed.content + input;
        } else {
          // Pasting in suffix
          prefixText = collapsed.prefixText + collapsed.content + input.slice(0, localPos);
          suffixText = input.slice(localPos);
        }
      } else {
        prefixText = input.slice(0, cursor);
        suffixText = input.slice(cursor);
      }

      setCollapsed({
        content: normalized,
        lineCount: result.lineCount,
        charCount: normalized.length,
        prefixText,
      });
      setInput(suffixText);
      setCursor(prefixText.length + 1);
      return true;
    }

    // Small paste - insert inline
    const { region, localPos } = getCursorInfo();
    if (collapsed) {
      if (region === 'prefix') {
        const newPrefix = collapsed.prefixText.slice(0, localPos) + normalized + collapsed.prefixText.slice(localPos);
        setCollapsed({ ...collapsed, prefixText: newPrefix });
        setCursor(cursor + normalized.length);
      } else {
        const newInput = input.slice(0, localPos) + normalized + input.slice(localPos);
        setInput(newInput);
        setCursor(cursor + normalized.length);
      }
    } else {
      const newInput = input.slice(0, cursor) + normalized + input.slice(cursor);
      updateInput(newInput, cursor + normalized.length);
    }
    return false;
  };

  useKeypress((userInput: string, key: Key) => {
    if (isProcessing) return;

    if (key.paste) {
      handlePaste(userInput);
      return;
    }

    if (key.return) {
      const content = buildFullContent(collapsed, input);
      if (content) {
        if (clearOnSubmit) {
          clearAll();
        }
        onSubmit(content);
      }
    } else if (key.backspace || key.delete) {
      if (collapsed) {
        const { region, localPos } = getCursorInfo();
        const prefixLen = collapsed.prefixText.length;

        if (region === 'prefix' && localPos > 0) {
          // Delete char in prefix
          const newPrefix = collapsed.prefixText.slice(0, localPos - 1) + collapsed.prefixText.slice(localPos);
          setCollapsed({ ...collapsed, prefixText: newPrefix });
          setCursor(cursor - 1);
        } else if (cursor === prefixLen + 1) {
          // Cursor right after chip - delete the chip
          const newInput = collapsed.prefixText + input;
          setCollapsed(null);
          setInput(newInput);
          setCursor(prefixLen);
          lastValueRef.current = newInput;
          onInputChange?.(newInput);
        } else if (region === 'suffix' && localPos > 0) {
          // Delete char in suffix
          const newInput = input.slice(0, localPos - 1) + input.slice(localPos);
          setInput(newInput);
          setCursor(cursor - 1);
        }
      } else if (cursor > 0) {
        updateInput(
          input.slice(0, cursor - 1) + input.slice(cursor),
          cursor - 1
        );
      }
    } else if (key.leftArrow) {
      setCursor(Math.max(0, cursor - 1));
    } else if (key.rightArrow) {
      setCursor(Math.min(getTotalLength(), cursor + 1));
    } else if (key.home) {
      setCursor(0);
    } else if (key.end) {
      setCursor(getTotalLength());
    } else if (key.upArrow || key.downArrow || key.pageUp || key.pageDown) {
      // Ignore navigation keys
    } else if (key.ctrl && userInput === 'j') {
      // Ctrl+J inserts a newline
      insertText('\n');
    } else if (!key.ctrl && !key.meta && userInput && isPrintable(userInput)) {
      insertText(normalizeLineEndings(userInput));
    }
  });

  const insertText = (text: string) => {
    if (collapsed) {
      const { region, localPos } = getCursorInfo();
      if (region === 'prefix') {
        const newPrefix = collapsed.prefixText.slice(0, localPos) + text + collapsed.prefixText.slice(localPos);
        setCollapsed({ ...collapsed, prefixText: newPrefix });
        setCursor(cursor + text.length);
      } else {
        const newInput = input.slice(0, localPos) + text + input.slice(localPos);
        setInput(newInput);
        setCursor(cursor + text.length);
      }
    } else {
      const newInput = input.slice(0, cursor) + text + input.slice(cursor);
      updateInput(newInput, cursor + text.length);
    }
  };

  const renderCursor = () => <Text>{getColor('primary')('❙')}</Text>;

  const renderText = (text: string, cursorPos: number | null, dimAfterCursor = false) => {
    if (cursorPos === null) {
      return <Text>{text}</Text>;
    }
    return (
      <>
        <Text>{text.slice(0, cursorPos)}</Text>
        {renderCursor()}
        {dimAfterCursor ? (
          <Text>{getColor('secondary')(text.slice(cursorPos))}</Text>
        ) : (
          <Text>{text.slice(cursorPos)}</Text>
        )}
      </>
    );
  };

  const renderContent = () => {
    if (!collapsed) {
      // No collapsed content - simple case
      if (input.length === 0) {
        return (
          <>
            {renderCursor()}
            <Text>{getColor('muted')(placeholder)}</Text>
          </>
        );
      }

      // Handle multi-line
      if (input.includes('\n')) {
        const lines = input.split('\n');
        let charCount = 0;
        return (
          <Box flexDirection="column">
            {lines.map((line, index) => {
              const lineStart = charCount;
              const lineEnd = charCount + line.length;
              charCount = lineEnd + 1; // +1 for the newline

              const cursorInLine = cursor >= lineStart && cursor <= lineEnd;
              const cursorPosInLine = cursor - lineStart;

              return (
                <Box key={index}>
                  {cursorInLine ? (
                    <>
                      <Text>{line.slice(0, cursorPosInLine)}</Text>
                      {renderCursor()}
                      <Text>{getColor('secondary')(line.slice(cursorPosInLine))}</Text>
                    </>
                  ) : (
                    <Text>{line}</Text>
                  )}
                </Box>
              );
            })}
          </Box>
        );
      }

      return renderText(input, cursor, true);
    }

    // Has collapsed content
    const { region, localPos } = getCursorInfo();

    return (
      <>
        {region === 'prefix' ? (
          renderText(collapsed.prefixText, localPos)
        ) : (
          collapsed.prefixText && <Text>{collapsed.prefixText}</Text>
        )}
        <PastedChip
          lineCount={collapsed.lineCount}
          charCount={collapsed.charCount}
        />
        <Text> </Text>
        {region === 'suffix' ? (
          input.length === 0 ? (
            renderCursor()
          ) : (
            renderText(input, localPos, true)
          )
        ) : (
          input && <Text>{input}</Text>
        )}
      </>
    );
  };

  return <Box>{renderContent()}</Box>;
});
