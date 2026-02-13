import React from 'react';
import { Write } from './Write.js';
import { Card } from '../../ui/card/Card.js';
import { StatusBar } from '../status-bar/StatusBar.js';

const meta = {
  component: Write,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Writing', 'Wrote', 'Created', 'Replaced', 'Inserted'],
  },
  tags: ['autodocs'],
};

export default meta;

const oldCode = `function greet(name) {
  console.log("Hello " + name);
}`;

const newCode = `function greet(name) {
  console.log("Hello, " + name + "!");
}`;

const newFileContent = `import { useState } from 'react';

export function useCounter(initial = 0) {
  const [count, setCount] = useState(initial);
  return { count, increment: () => setCount(c => c + 1) };
}`;

// Writing in progress (shimmer, no diff summary)
export const Writing = {
  render: () => (
    <Card active={true}>
      <StatusBar>
        <Write
          oldText=""
          newText=""
          content={JSON.stringify({
            command: 'strReplace',
            path: 'src/utils/helpers.ts',
            oldStr: oldCode,
            newStr: newCode,
          })}
          isFinished={false}
        />
      </StatusBar>
    </Card>
  ),
};

// Wrote (strReplace finished)
export const Wrote = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
          oldText=""
          newText=""
          content={JSON.stringify({
            command: 'strReplace',
            path: 'src/utils/helpers.ts',
            oldStr: oldCode,
            newStr: newCode,
          })}
          isFinished={true}
        />
      </StatusBar>
    </Card>
  ),
};

// Created a new file
export const Created = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
          oldText=""
          newText=""
          content={JSON.stringify({
            command: 'create',
            path: 'src/hooks/useCounter.ts',
            content: newFileContent,
          })}
          isFinished={true}
        />
      </StatusBar>
    </Card>
  ),
};

// Replaced in file
export const Replaced = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
          oldText=""
          newText=""
          content={JSON.stringify({
            command: 'strReplace',
            path: 'src/components/Button.tsx',
            oldStr: `const Button = ({ label }) => {
  return <button>{label}</button>;
};`,
            newStr: `const Button = ({ label, onClick, disabled = false }) => {
  return (
    <button onClick={onClick} disabled={disabled}>
      {label}
    </button>
  );
};`,
          })}
          isFinished={true}
        />
      </StatusBar>
    </Card>
  ),
};

// Inserted at a specific line
export const Inserted = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
          oldText=""
          newText=""
          content={JSON.stringify({
            command: 'insert',
            path: 'src/index.ts',
            insertLine: 5,
            content: `import { useCounter } from './hooks/useCounter';`,
          })}
          isFinished={true}
        />
      </StatusBar>
    </Card>
  ),
};
