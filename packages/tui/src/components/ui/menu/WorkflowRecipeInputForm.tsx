import React, { useMemo, useReducer, useRef, useState } from 'react';
import { Box, Input } from '../../../renderer.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useKeypress, keyToRawBytes } from '../../../hooks/useKeypress.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import type { WorkflowRecipeDescriptor } from '../../../types/workflow-launch.js';
import { Divider } from '../divider/Divider.js';
import { Text } from '../text/Text.js';

interface WorkflowRecipeInputFormProps {
  recipe: WorkflowRecipeDescriptor;
  initialValues: Record<string, string>;
  onSubmit: (values: Record<string, string>) => void;
  onCancel: () => void;
}

function defaultInputHint(kind: string): string {
  switch (kind) {
    case 'prompt':
      return 'Describe the task or goal';
    case 'file':
      return 'Enter a file path';
    default:
      return 'Enter a value';
  }
}

export function WorkflowRecipeInputForm({
  recipe,
  initialValues,
  onSubmit,
  onCancel,
}: WorkflowRecipeInputFormProps) {
  const fields = useMemo(
    () => Object.entries(recipe.inputs ?? {}),
    [recipe.inputs]
  );
  const { width } = useTerminalSize();
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const primary = getColor('primary');
  const secondary = getColor('secondary');
  const brand = getColor('brand');
  const errorColor = getColor('error');

  const [activeIndex, setActiveIndex] = useState(0);
  const activeIndexRef = useRef(0);
  const [values, setValues] = useState<Record<string, string>>(() => ({
    ...initialValues,
  }));
  const valuesRef = useRef(values);
  const [error, setError] = useState<string | null>(null);
  const [, rerenderInput] = useReducer((value) => value + 1, 0);
  const inputRef = useRef<Input | null>(null);

  if (!inputRef.current) {
    const nextInput = new Input();
    nextInput.focused = true;
    nextInput.setValue(valuesRef.current[fields[0]?.[0] ?? ''] ?? '');
    nextInput.handleInput('\x1b[F');
    inputRef.current = nextInput;
  }

  const input = inputRef.current;
  input.onChange = (value) => {
    const fieldName = fields[activeIndexRef.current]?.[0];
    if (!fieldName) return;
    const next = { ...valuesRef.current, [fieldName]: value };
    valuesRef.current = next;
    setValues(next);
    setError(null);
  };

  const focusField = (index: number) => {
    if (fields.length === 0) return;
    const nextIndex = (index + fields.length) % fields.length;
    activeIndexRef.current = nextIndex;
    setActiveIndex(nextIndex);
    input.setValue(valuesRef.current[fields[nextIndex]![0]] ?? '');
    input.handleInput('\x1b[F');
    setError(null);
    rerenderInput();
  };

  const submit = () => {
    const missingIndex = fields.findIndex(
      ([name]) => !valuesRef.current[name]?.trim()
    );
    if (missingIndex !== -1) {
      focusField(missingIndex);
      setError(`${fields[missingIndex]![0]} is required`);
      return;
    }
    onSubmit(valuesRef.current);
  };

  useKeypress((typed, key) => {
    if (key.escape) {
      onCancel();
      return;
    }
    if (key.upArrow || (key.tab && key.shift)) {
      focusField(activeIndexRef.current - 1);
      return;
    }
    if (key.downArrow || key.tab) {
      focusField(activeIndexRef.current + 1);
      return;
    }
    if (key.return) {
      if (activeIndexRef.current < fields.length - 1) {
        const currentName = fields[activeIndexRef.current]![0];
        if (!valuesRef.current[currentName]?.trim()) {
          setError(`${currentName} is required`);
          return;
        }
        focusField(activeIndexRef.current + 1);
      } else {
        submit();
      }
      return;
    }
    if (key.ctrl || key.meta) return;
    input.handleInput(keyToRawBytes(key, typed));
    rerenderInput();
  });

  const activeInput = input.render(Math.max(8, width - 8))[0] ?? '';
  const enterAction =
    fields.length === 0 || activeIndex === fields.length - 1 ? 'run' : 'next';
  const footerHint =
    fields.length === 0
      ? `${glyphs.enter} run ${glyphs.smallDot} esc cancel`
      : `${glyphs.arrowUp}${glyphs.arrowDown} fields ${glyphs.smallDot} ${glyphs.enter} ${enterAction} ${glyphs.smallDot} esc cancel`;

  return (
    <Box flexDirection="column">
      <Box paddingX={1} flexDirection="column">
        <Text>{brand(`Run workflow ${glyphs.smallDot} ${recipe.name}`)}</Text>
        {recipe.description && (
          <Text wrap="wrap">{secondary(recipe.description)}</Text>
        )}
      </Box>
      <Divider />
      <Box paddingX={1} flexDirection="column">
        {fields.length === 0 && <Text>{secondary('No inputs required.')}</Text>}
        {fields.map(([name, kind], index) => {
          const active = index === activeIndex;
          const value = values[name] ?? '';
          return (
            <Box key={name} flexDirection="column" marginBottom={1}>
              <Text wrap="truncate">
                {active
                  ? brand(`${glyphs.chevron} ${name}`)
                  : primary(`  ${name}`)}
                {secondary(
                  `  ${kind} ${glyphs.smallDot} ${defaultInputHint(kind)}`
                )}
              </Text>
              <Box paddingLeft={2}>
                <Text wrap="truncate">
                  {active
                    ? activeInput
                    : value
                      ? value
                      : secondary('> required')}
                </Text>
              </Box>
            </Box>
          );
        })}
        {error && <Text>{errorColor(error)}</Text>}
        <Text>{secondary(footerHint)}</Text>
      </Box>
    </Box>
  );
}
