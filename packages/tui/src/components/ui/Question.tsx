import React, { useLayoutEffect, useReducer, useRef, useState } from 'react';
import {
  Box,
  CURSOR_MARKER,
  Input,
  useTwinkiContext,
} from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTextStyle } from '../../hooks/useTextStyle.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { isPrintable, stripNonPrintable } from '../../utils/string.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import { Panel } from './panel/Panel.js';
import type { UserInputOption } from '@kiro/acp-type-covenant';

export interface QuestionProps {
  question: string;
  options: UserInputOption[];
  onAnswer: (answer: string, answerForAgent?: string) => boolean;
  onCancel: () => void;
  titlePrefix?: string;
}

type SubOptionPage = {
  optionIndex: number;
  focused: number;
  selected: Set<number>;
};

type NumberShortcut = {
  prefix: string;
  answer: string;
};

const FREE_TEXT_LABEL = 'Type a different answer…';

function normalizePaste(value: string): string {
  return stripNonPrintable(value).replace(/\r\n|\r|\n/g, ' ');
}

function resolveNumberedAnswer(
  answer: string,
  shortcut: NumberShortcut | null
): string {
  if (!matchesNumberShortcut(answer, shortcut)) return answer;
  const suffix = answer.slice(shortcut.prefix.length);
  return `${shortcut.answer}${suffix}`;
}

function matchesNumberShortcut(
  answer: string,
  shortcut: NumberShortcut | null
): shortcut is NumberShortcut {
  if (!shortcut || !answer.startsWith(shortcut.prefix)) return false;
  const suffix = answer.slice(shortcut.prefix.length);
  return suffix === '' || /^\s/.test(suffix);
}

const QuestionTextInput: React.FC<{ input: Input }> = ({ input }) => {
  const { width } = useTerminalSize();
  const rendered = (input.render(Math.max(1, width - 2))[0] ?? '').slice(2);
  const secondary = useTheme().getColor('secondary');
  return (
    <Text>
      {input.getValue()
        ? rendered
        : `${rendered.trimEnd()} ${secondary('type your answer…')}`}
    </Text>
  );
};

export const Question: React.FC<QuestionProps> = ({
  question,
  options,
  onAnswer,
  onCancel,
  titlePrefix = '',
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { tui } = useTwinkiContext();
  const primary = getColor('primary');
  const secondary = getColor('secondary');
  const selectedLabel = useTextStyle('selectedLabel');

  const freeTextIndex = options.length;
  const initialFocus = Math.max(
    0,
    options.findIndex((option) => option.recommended)
  );
  const [focused, setFocused] = useState(initialFocus);
  const focusedRef = useRef(initialFocus);
  const [editing, setEditing] = useState(false);
  const editingRef = useRef(false);
  const [subOptionPage, setSubOptionPage] = useState<SubOptionPage | null>(
    null
  );
  const subOptionPageRef = useRef<SubOptionPage | null>(null);
  const resolvedRef = useRef(false);
  const shortcutRef = useRef('');
  const numberShortcutRef = useRef<NumberShortcut | null>(null);
  const inputRef = useRef<Input | null>(null);
  const submitInputRef = useRef<(value: string) => void>(() => {});
  const [, rerenderInput] = useReducer((value) => value + 1, 0);

  const submitAnswer = (value: string, answerForAgent?: string) => {
    if (resolvedRef.current) return false;
    const accepted =
      answerForAgent === undefined
        ? onAnswer(value)
        : onAnswer(value, answerForAgent);
    if (accepted) resolvedRef.current = true;
    return accepted;
  };

  const openFreeText = (
    initialValue = '',
    pasted = false,
    numberShortcutPrefix?: string
  ) => {
    shortcutRef.current = '';
    if (!editingRef.current) {
      const option = numberShortcutPrefix
        ? options[Number(numberShortcutPrefix) - 1]
        : undefined;
      numberShortcutRef.current =
        option && numberShortcutPrefix
          ? { prefix: numberShortcutPrefix, answer: option.title }
          : null;
      const input = new Input();
      input.focused = true;
      input.onSubmit = (value) => submitInputRef.current(value);
      inputRef.current = input;
      editingRef.current = true;
      setEditing(true);
    }
    if (initialValue) {
      inputRef.current?.handleInput(
        pasted
          ? `\x1b[200~${normalizePaste(initialValue)}\x1b[201~`
          : initialValue
      );
      rerenderInput();
    }
  };

  const closeFreeText = () => {
    editingRef.current = false;
    if (inputRef.current) {
      inputRef.current.focused = false;
      inputRef.current.onSubmit = undefined;
    }
    inputRef.current = null;
    numberShortcutRef.current = null;
    setEditing(false);
  };

  submitInputRef.current = (value) => {
    if (!value.trim() || resolvedRef.current) return;
    const resolved = resolveNumberedAnswer(value, numberShortcutRef.current);
    if (submitAnswer(value, resolved === value ? undefined : resolved)) {
      closeFreeText();
    }
  };

  useLayoutEffect(
    () =>
      tui.addInputListener((data) => {
        if (!editingRef.current || resolvedRef.current) return;
        inputRef.current?.handleInput(data);
        rerenderInput();
      }),
    [tui]
  );

  const handleClose = () => {
    if (resolvedRef.current) return;
    if (editingRef.current) {
      closeFreeText();
    } else if (subOptionPageRef.current) {
      subOptionPageRef.current = null;
      setSubOptionPage(null);
    } else {
      onCancel();
    }
  };

  useKeypress((input, key) => {
    if (resolvedRef.current) return;
    const pastedInput = key.paste ? stripNonPrintable(input) : '';
    const plainInput =
      !!input &&
      !key.paste &&
      isPrintable(input) &&
      !key.ctrl &&
      !key.meta &&
      !key.return &&
      !key.tab;

    if (editingRef.current) {
      if (key.paste && pastedInput) openFreeText(pastedInput, true);
      else if (key.ctrl && input === 'c') onCancel();
      return;
    }

    const activePage = subOptionPageRef.current;
    if (activePage) {
      const option = options[activePage.optionIndex];
      const subOptions = option?.subOptions ?? [];
      if (key.paste && pastedInput) {
        subOptionPageRef.current = null;
        setSubOptionPage(null);
        openFreeText(pastedInput, true);
      } else if (plainInput && input !== ' ') {
        subOptionPageRef.current = null;
        setSubOptionPage(null);
        openFreeText(input);
      } else if (key.upArrow || key.downArrow) {
        const delta = key.upArrow ? -1 : 1;
        const next = {
          ...activePage,
          focused:
            (activePage.focused + delta + subOptions.length + 1) %
            (subOptions.length + 1),
        };
        subOptionPageRef.current = next;
        setSubOptionPage(next);
      } else if (
        (input === ' ' || key.return) &&
        activePage.focused < subOptions.length
      ) {
        const selected = new Set(activePage.selected);
        if (selected.has(activePage.focused)) {
          selected.delete(activePage.focused);
        } else {
          selected.add(activePage.focused);
        }
        const next = { ...activePage, selected };
        subOptionPageRef.current = next;
        setSubOptionPage(next);
      } else if (key.return && option) {
        const selected = subOptions
          .filter((_, index) => activePage.selected.has(index))
          .map((subOption) => subOption.title);
        submitAnswer(`${option.title} [${selected.join(', ')}]`);
      } else if (key.ctrl && input === 'c') {
        onCancel();
      }
      return;
    }

    const rows = options.length + 1;
    if (key.upArrow || key.downArrow) {
      shortcutRef.current = '';
      const delta = key.upArrow ? -1 : 1;
      focusedRef.current = (focusedRef.current + delta + rows) % rows;
      setFocused(focusedRef.current);
    } else if (key.backspace) {
      shortcutRef.current = '';
    } else if (key.paste && pastedInput) {
      const prefix = shortcutRef.current || undefined;
      openFreeText(`${shortcutRef.current}${pastedInput}`, true, prefix);
    } else if (plainInput) {
      for (const character of Array.from(input)) {
        if (editingRef.current) {
          openFreeText(character);
          continue;
        }
        if (focusedRef.current === freeTextIndex) {
          openFreeText(character);
          continue;
        }
        if (!/^\d$/.test(character)) {
          const prefix = shortcutRef.current || undefined;
          openFreeText(`${shortcutRef.current}${character}`, false, prefix);
          continue;
        }

        const optionIndex = Number(character) - 1;
        if (optionIndex >= 0 && optionIndex < options.length) {
          shortcutRef.current = character;
          focusedRef.current = optionIndex;
          setFocused(optionIndex);
        } else {
          openFreeText(`${shortcutRef.current}${character}`);
        }
      }
    } else if (key.return) {
      shortcutRef.current = '';
      if (focusedRef.current === freeTextIndex) {
        openFreeText();
        return;
      }
      const option = options[focusedRef.current];
      if (!option) return;
      if (option.subOptions?.length) {
        const next = {
          optionIndex: focusedRef.current,
          focused: option.subOptions.length,
          selected: new Set(option.subOptions.map((_, index) => index)),
        };
        subOptionPageRef.current = next;
        setSubOptionPage(next);
      } else {
        submitAnswer(option.title);
      }
    } else if (key.ctrl && input === 'c') {
      onCancel();
    }
  });

  const row = (
    label: string,
    index: number,
    activeIndex = editing ? freeTextIndex : focused
  ) =>
    index === activeIndex
      ? `${CURSOR_MARKER}${primary(glyphs.chevron)} ${selectedLabel(label)}`
      : `  ${label}`;
  const activeOption = subOptionPage
    ? options[subOptionPage.optionIndex]
    : undefined;
  const activeSubOptions = activeOption?.subOptions ?? [];

  return (
    <Panel
      title={`${titlePrefix}Question`}
      onClose={handleClose}
      showTabHint={false}
      hideTitleDivider={true}
      footerIndent={2}
      footerLeft={
        <Text>
          {primary(`${glyphs.arrowUp}${glyphs.arrowDown}`)}{' '}
          {secondary('to navigate')}
          {subOptionPage && (
            <>
              {secondary(` ${glyphs.smallDot} `)}
              {primary('space')} {secondary('to toggle')}
            </>
          )}
          {secondary(` ${glyphs.smallDot} `)}
          {primary(glyphs.enter)} {secondary('to submit')}
        </Text>
      }
      closeHintLabel={subOptionPage ? 'to pick a choice' : 'to cancel'}
    >
      <Box flexDirection="column">
        {question && (
          <Box marginBottom={1}>
            <MarkdownRenderer content={question} color={primary} />
          </Box>
        )}
        {subOptionPage ? (
          <>
            {activeOption?.subOptionsLabel && (
              <Text>{secondary(activeOption.subOptionsLabel)}</Text>
            )}
            {activeSubOptions.map((subOption, index) => (
              <React.Fragment key={`${index}:${subOption.title}`}>
                <Text>
                  {row(
                    `[${subOptionPage.selected.has(index) ? glyphs.checkmark : ' '}] ${subOption.title}`,
                    index,
                    subOptionPage.focused
                  )}
                </Text>
                {subOption.description && (
                  <Box paddingLeft={6}>
                    <Text>{secondary(subOption.description)}</Text>
                  </Box>
                )}
              </React.Fragment>
            ))}
            <Text>
              {row(
                'Submit answer',
                activeSubOptions.length,
                subOptionPage.focused
              )}
            </Text>
          </>
        ) : (
          options.map((option, index) => (
            <React.Fragment key={`${index}:${option.title}`}>
              <Text>
                {row(`${index + 1}. ${option.title}`, index)}
                {option.recommended ? secondary(' (recommended)') : ''}
              </Text>
              {option.description && (
                <Box paddingLeft={5}>
                  <Text>{secondary(option.description)}</Text>
                </Box>
              )}
            </React.Fragment>
          ))
        )}
        {!subOptionPage && editing ? (
          <Box>
            <Text>{primary(`${glyphs.chevron} `)}</Text>
            {inputRef.current && <QuestionTextInput input={inputRef.current} />}
          </Box>
        ) : !subOptionPage ? (
          <Text>{row(FREE_TEXT_LABEL, freeTextIndex)}</Text>
        ) : null}
      </Box>
    </Panel>
  );
};
