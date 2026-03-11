/**
 * 23-bg-colors.tsx — Chat messages colored via chalk (same pattern as Message.tsx)
 *
 * Run: npx tsx examples/23-bg-colors.tsx
 */
import React from 'react';
import { render, Text, Box } from 'twinki';
import chalk from 'chalk';

type MessageType = 'user' | 'agent';

interface MessageProps {
  type: MessageType;
  content: string;
}

const Message = ({ type, content }: MessageProps) => {
  if (type === 'agent') {
    return (
      <Box marginBottom={1}>
        {/* chalk applies ANSI color directly to the string — no color prop needed */}
        <Text>{chalk.cyan(content)}</Text>
      </Box>
    );
  }

  // User messages: chalk colors the text, Box provides the background
  return (
    <Box marginBottom={1}>
      <Box backgroundColor="#1e1e2e" paddingX={1}>
        <Text wrap="wrap">{chalk.white(content)}</Text>
      </Box>
    </Box>
  );
};

const messages: { type: MessageType; content: string }[] = [
  { type: 'user', content: 'What is the capital of France?' },
  { type: 'agent', content: 'The capital of France is Paris.' },
  { type: 'user', content: 'And what about Germany?' },
  { type: 'agent', content: 'The capital of Germany is Berlin.' },
  { type: 'user', content: 'Thanks!' },
  { type: 'agent', content: 'You are welcome.' },
];

const App = () => (
  <Box flexDirection="column" padding={1}>
    {messages.map((msg, i) => (
      <Message key={i} type={msg.type} content={msg.content} />
    ))}
    <Text dimColor>Press Ctrl+C to exit</Text>
  </Box>
);

render(<App />);
