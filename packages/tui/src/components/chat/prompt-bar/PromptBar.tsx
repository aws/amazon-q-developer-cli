import React from 'react';
import { Box, Text } from './../../../renderer.js';
import { ContextBar } from './ContextBar.js';
import { PromptInput, type PromptInputProps } from './PromptInput.js';
import { Divider } from '../../ui/divider/Divider.js';
import { SnackBar } from './SnackBar.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';

// Detect terminals that fill edge margins for visual consistency
const fillsEdgeMargin = process.env.TERM_PROGRAM === 'iTerm.app';

// Type-safe header that only accepts ContextBar or SnackBar components
export type PromptBarHeader =
  | React.ReactElement<
      React.ComponentProps<typeof ContextBar>,
      typeof ContextBar
    >
  | React.ReactElement<React.ComponentProps<typeof SnackBar>, typeof SnackBar>;

interface PromptBarProps {
  header?: PromptBarHeader;
  subHeader?: React.ReactNode;
  children?: React.ReactNode;
  /**
   * Receives the prompt for the model and, when they differ, the shorter form
   * to echo in the transcript — an image chip stands for a path the user should
   * not have to read back.
   */
  onSubmit: (
    command: string,
    displayCommand?: string,
    carriesAttachments?: boolean
  ) => void;
  isProcessing: boolean;
  triggerRules?: Array<{ key: string; type: 'start' | 'inline' }>;
  onTriggerDetected?: (trigger: any) => void;
  onInputChange?: (value: string) => void;
  placeholder?: string;
  clearOnSubmit?: boolean;
  value?: string;
  hint?: string;
  hideInput?: boolean;
  isInputOwnedExternally?: PromptInputProps['isInputOwnedExternally'];
}

export const PromptBar = React.memo(function PromptBar({
  header,
  subHeader,
  children,
  onSubmit,
  isProcessing,
  triggerRules,
  onTriggerDetected,
  placeholder,
  hint,
  hideInput = false,
  isInputOwnedExternally,
}: PromptBarProps) {
  const glyphs = useGlyphs();
  const resolvedPlaceholder =
    placeholder ?? `ask a question, or describe a task ${glyphs.enter}`;
  return (
    <Box flexDirection="column" gap={0}>
      <Divider />
      <Box
        paddingLeft={fillsEdgeMargin ? 1 : 0}
        flexDirection="column"
        width="100%"
      >
        {header && <Box marginBottom={1}>{header}</Box>}
        {subHeader}
        {!hideInput && (
          <Box>
            <PromptInput
              onSubmit={onSubmit}
              isProcessing={isProcessing}
              triggerRules={triggerRules}
              onTriggerDetected={onTriggerDetected}
              placeholder={resolvedPlaceholder}
              isInputOwnedExternally={isInputOwnedExternally}
            />
            {hint && <Text dimColor> {hint}</Text>}
          </Box>
        )}
        {children}
      </Box>
    </Box>
  );
});
