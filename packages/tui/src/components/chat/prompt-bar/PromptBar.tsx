import React from 'react';
import { Box, Text } from 'ink';
import { ContextBar } from './ContextBar.js';
import { PromptInput } from './PromptInput.js';
import { Divider } from '../../ui/divider/Divider.js';
import { SnackBar } from './SnackBar.js';

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
  onSubmit: (command: string) => void;
  isProcessing: boolean;
  triggerRules?: Array<{ key: string; type: 'start' | 'inline' }>;
  onTriggerDetected?: (trigger: any) => void;
  onInputChange?: (value: string) => void;
  placeholder?: string;
  clearOnSubmit?: boolean;
  value?: string;
  hint?: string;
  hideInput?: boolean;
}

export function PromptBar({
  header,
  subHeader,
  children,
  onSubmit,
  isProcessing,
  triggerRules,
  onTriggerDetected,
  placeholder = 'ask a question, or describe a task ↵',
  hint,
  hideInput = false,
}: PromptBarProps) {
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
              placeholder={placeholder}
            />
            {hint && <Text dimColor> {hint}</Text>}
          </Box>
        )}
        {children}
      </Box>
    </Box>
  );
}
