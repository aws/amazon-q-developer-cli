import React, { useCallback, useState } from 'react';
import { Box, useInput } from './../../renderer.js';
import { Text } from './text/Text.js';
import { Divider } from './divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTextStyle } from '../../hooks/useTextStyle.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';
import type {
  SurveyDefinition,
  SurveyQuestion,
} from '../../constants/survey.js';
import { useAppStore } from '../../stores/app-store.js';
import { visibleWidth } from '../../utils/text-width.js';
import { isPrintable } from '../../utils/string.js';

interface SurveyPanelProps {
  onClose: () => void;
  onSubmit: (answers: Record<string, string>) => void;
}

/**
 * Survey panel — renders inline below the divider, above the prompt bar.
 *
 * Does NOT use the Panel wrapper (which has its own useInput that conflicts
 * with our text input handling). Instead, we build a minimal layout and
 * handle all input in a single useInput hook.
 */
export const SurveyPanel: React.FC<SurveyPanelProps> = ({
  onClose,
  onSubmit,
}) => {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const errorColor = getColor('error');
  const selectedLabel = useTextStyle('selectedLabel');

  const activeSurvey = useAppStore((s) => s.activeSurvey);
  const survey = activeSurvey as SurveyDefinition;
  const questions = survey?.questions ?? [];
  const [index, setIndex] = useState(0);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [freeText, setFreeText] = useState('');
  const [validationError, setValidationError] = useState<string | null>(null);
  const [selectedChoice, setSelectedChoice] = useState<string | undefined>(
    undefined
  );

  const question = questions[index] as SurveyQuestion | undefined;
  const isLast = index === questions.length - 1;

  const commitAndAdvance = useCallback(
    (value: string) => {
      if (!question) return;
      const next = { ...answers, [question.id]: value };
      if (isLast) {
        onSubmit(next);
      } else {
        setAnswers(next);
        setIndex((i) => i + 1);
        setFreeText('');
        setValidationError(null);
        setSelectedChoice(undefined);
      }
    },
    [answers, isLast, onSubmit, question]
  );

  // Single useInput handler for ALL input — no Panel wrapper competing.
  // LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 38 to the 30 allowed.; refactor before extending
  // eslint-disable-next-line sonarjs/cognitive-complexity
  useInput((input, key) => {
    if (!question) return;

    // Escape closes the panel (same as Panel's ESC behavior)
    if (key.escape) {
      onClose();
      return;
    }

    const currentIsRating = question.responseType === 'rating';

    if (currentIsRating) {
      // Arrow keys for RadioGroup navigation
      if (key.upArrow || key.downArrow) {
        const opts = question.options ?? [];
        if (opts.length === 0) return;
        const current = selectedChoice ?? opts[0]!;
        const idx = opts.indexOf(current);
        if (key.upArrow) {
          const prev = idx > 0 ? idx - 1 : opts.length - 1;
          setSelectedChoice(opts[prev]);
        } else {
          const next = idx < opts.length - 1 ? idx + 1 : 0;
          setSelectedChoice(opts[next]);
        }
        return;
      }
      // Enter submits the selection
      if (key.return) {
        const chosen = selectedChoice ?? question.options?.[0];
        if (chosen !== undefined) {
          commitAndAdvance(chosen);
        }
      }
      return;
    }

    // text / textArea input handling
    if (key.return) {
      const trimmed = freeText.trim();
      if (trimmed.length === 0 && question.optional) {
        commitAndAdvance('');
        return;
      }
      const validationMessage = question.validate?.(freeText) ?? null;
      if (validationMessage !== null) {
        setValidationError(validationMessage);
        return;
      }
      setValidationError(null);
      commitAndAdvance(trimmed);
      return;
    }
    if (key.backspace || key.delete) {
      setFreeText((t) => t.slice(0, -1));
      if (validationError) setValidationError(null);
      return;
    }
    if (input && isPrintable(input) && !key.ctrl && !key.meta) {
      setFreeText((t) => t + input);
      if (validationError) setValidationError(null);
    }
  });

  if (!question) {
    return null;
  }

  return (
    <Box flexDirection="column" width={termWidth}>
      <Box paddingX={1} justifyContent="space-between">
        <Text>{primary(survey?.title ?? 'Survey')}</Text>
        <Text>{dim(`${index + 1} of ${questions.length} questions`)}</Text>
      </Box>
      <Divider />

      <Box flexDirection="column" paddingX={1}>
        <Box marginBottom={1}>
          <Text>{question.prompt}</Text>
        </Box>

        {question.responseType === 'rating' && question.options ? (
          <Box flexDirection="column">
            {question.options.map((label) => {
              const isSelected =
                (selectedChoice ?? question.options![0]) === label;
              return (
                <Box key={label}>
                  <Text>
                    {isSelected
                      ? selectedLabel('> ' + label)
                      : dim('  ' + label)}
                  </Text>
                </Box>
              );
            })}
          </Box>
        ) : (
          <Box flexDirection="column">
            <Box>
              <Text>{primary('> ')}</Text>
              {freeText.length > 0 ? (
                <Text>{freeText}</Text>
              ) : (
                <Text>{dim(question.placeholder ?? '')}</Text>
              )}
              <Text inverse> </Text>
              {freeText.length + visibleWidth(question.placeholder ?? '') ===
                0 && <Text> </Text>}
            </Box>
            {validationError && (
              <Box marginTop={1}>
                <Text>{errorColor(validationError)}</Text>
              </Box>
            )}
          </Box>
        )}
      </Box>

      <Divider />
      <Box paddingX={1}>
        <Text>
          {primary('enter')} {dim('to select and proceed')} {dim('|')}{' '}
          {primary('esc')} {dim('to cancel and close')}
        </Text>
      </Box>
    </Box>
  );
};
