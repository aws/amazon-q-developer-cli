import React from 'react';
import { Box } from 'ink';
import { Text } from './text/Text.js';
import { Panel } from './panel/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';

interface PromptArg {
  name: string;
  description?: string;
  required?: boolean;
}

interface Prompt {
  name: string;
  description?: string;
  arguments: PromptArg[];
  serverName: string;
}

interface PromptsPanelProps {
  prompts: Prompt[];
  onClose: () => void;
}

function truncate(str: string, max: number): string {
  if (str.length <= max) return str;
  return max > 3 ? str.slice(0, max - 3) + '...' : str.slice(0, max);
}

export const PromptsPanel = React.memo(function PromptsPanel({ prompts, onClose }: PromptsPanelProps) {
  const { getColor } = useTheme();
  const { width } = useTerminalSize();

  const primary = getColor('primary');
  const secondary = getColor('secondary');
  const muted = getColor('muted');
  const brand = getColor('brand');

  // Column widths
  const indent = 2;
  const usable = width - indent - 2;
  const nameCol = Math.min(35, Math.floor(usable * 0.25));
  const descCol = Math.floor(usable * 0.45);
  const argsCol = usable - nameCol - descCol;

  // Group prompts by server
  const grouped: Record<string, Prompt[]> = {};
  for (const p of prompts) {
    (grouped[p.serverName] ??= []).push(p);
  }

  const divider = '\u2500'.repeat(Math.min(width - 6, usable));

  return (
    <Panel title="/prompts" onClose={onClose}>
      <Box>
        <Text wrap="truncate">
          {'  '}
          {primary.bold('Prompt'.padEnd(nameCol))}
          {primary.bold('Description'.padEnd(descCol))}
          {primary.bold('Arguments (* = required)')}
        </Text>
      </Box>
      <Box>
        <Text>{'  '}{muted(divider)}</Text>
      </Box>

      {Object.entries(grouped).map(([server, serverPrompts]) => (
        <Box key={server} flexDirection="column" marginBottom={1}>
          <Text>{'  '}{brand(server + ':')}</Text>
          {serverPrompts.map((prompt) => {
            const name = truncate(`/${prompt.name}`, nameCol - 1);
            const descText = prompt.description
              ? truncate(prompt.description, descCol - 1)
              : '(file prompt)';
            const descStyled = prompt.description
              ? secondary(descText.padEnd(descCol))
              : muted(descText.padEnd(descCol));
            const args = prompt.arguments.length > 0
              ? truncate(
                  prompt.arguments
                    .map(a => a.required ? `${a.name}*` : a.name)
                    .join(', '),
                  argsCol
                )
              : '';

            return (
              <Box key={prompt.name}>
                <Text wrap="truncate">
                  {'  '}{primary(name.padEnd(nameCol))}{descStyled}{muted(args)}
                </Text>
              </Box>
            );
          })}
        </Box>
      ))}
    </Panel>
  );
});