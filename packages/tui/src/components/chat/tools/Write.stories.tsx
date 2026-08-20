import React from 'react';
import { Write } from './Write.js';
import { Card } from '../../ui/card/Card.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import type { StorybookParameters } from '../../../storybook/contracts.js';

const meta = {
  title: 'Tools/Write',
  component: Write,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      writing: { label: 'Write operation in progress' },
      replaced: { label: 'Completed replacement diff' },
      created: { label: 'Completed new-file diff' },
      'replaced-at-line': { label: 'Replacement diff with source line offset' },
      inserted: { label: 'Insertion diff at a requested line' },
      deleted: { label: 'Pure deletion without phantom additions' },
      appended: { label: 'Content appended to an existing file' },
      'long-diff-collapsed': {
        label: 'Long diff preview with hidden-line count',
      },
      'diff-expanded': {
        label: 'Long diff expanded through the output shortcut',
        gapType: 'integration-only',
        description:
          'Expansion is owned by the global tool-output controller, not Write.',
      },
    },
    storyOrder: [
      'Writing',
      'Wrote',
      'Created',
      'Replaced',
      'Inserted',
      'Deleted',
      'Appended',
      'LongDiffCollapsed',
    ],
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

function certification(
  readyText: string,
  assertions: NonNullable<
    NonNullable<StorybookParameters['certification']>['assertions']
  >,
  coversVisualStates: readonly string[],
  environment?: Readonly<Record<string, string>>
): StorybookParameters {
  return {
    coversVisualStates,
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport: { columns: 120, rows: 30 },
      assertions: {
        visible: assertions.visible,
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
      },
      ...(environment ? { environment } : {}),
    },
  };
}

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
  parameters: certification(
    'src/utils/helpers.ts',
    {
      visible: ['Write src/utils/helpers.ts'],
      hidden: ['added', 'removed', 'Hello,'],
    },
    ['writing']
  ),
};

// Wrote (strReplace finished)
export const Wrote = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
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
  parameters: certification(
    'src/utils/helpers.ts',
    {
      visible: [
        'Write src/utils/helpers.ts',
        'added 1 line, removed 1 line',
        'console.log("Hello " + name);',
        'console.log("Hello, " + name + "!");',
      ],
      ordered: ['removed 1 line', 'Hello " + name', 'Hello, " + name'],
    },
    ['replaced']
  ),
};

// Created a new file
export const Created = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
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
  parameters: certification(
    'src/hooks/useCounter.ts',
    {
      visible: [
        'Write src/hooks/useCounter.ts',
        'added 6 lines',
        '+  export function useCounter',
      ],
      hidden: ['removed'],
    },
    ['created']
  ),
};

// Replaced in file
export const Replaced = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
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
          startLine={42}
        />
      </StatusBar>
    </Card>
  ),
  parameters: certification(
    'src/components/Button.tsx',
    {
      visible: [
        'Write src/components/Button.tsx',
        'added 6 lines, removed 2 lines at L42',
        '42',
        'onClick={onClick}',
      ],
    },
    ['replaced-at-line']
  ),
};

// Inserted at a specific line
export const Inserted = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
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
  parameters: certification(
    'src/index.ts',
    {
      visible: [
        'Write src/index.ts',
        'added 1 line at L5',
        "import { useCounter } from './hooks/useCounter';",
      ],
      hidden: ['removed'],
    },
    ['inserted']
  ),
};

export const Deleted = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
          content={JSON.stringify({
            command: 'strReplace',
            path: 'src/legacy.ts',
            oldStr: 'const legacy = true;\nconst unused = true;\n',
            newStr: '',
          })}
          isFinished={true}
          startLine={12}
        />
      </StatusBar>
    </Card>
  ),
  parameters: certification(
    'src/legacy.ts',
    {
      visible: [
        'Write src/legacy.ts',
        'removed 2 lines at L12',
        '-  const legacy = true;',
        '-  const unused = true;',
      ],
      hidden: ['added'],
    },
    ['deleted']
  ),
};

export const Appended = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
          content={JSON.stringify({
            command: 'append',
            path: 'src/release-notes.md',
            new_str: '## Verification\n\nAll visual stories passed.',
          })}
          isFinished={true}
        />
      </StatusBar>
    </Card>
  ),
  parameters: certification(
    'src/release-notes.md',
    {
      visible: [
        'Write src/release-notes.md',
        'added 3 lines',
        'All visual stories passed.',
      ],
      hidden: ['removed'],
    },
    ['appended'],
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};

export const LongDiffCollapsed = {
  render: () => (
    <Card active={true}>
      <StatusBar status="success">
        <Write
          content={JSON.stringify({
            command: 'create',
            path: 'src/generated.ts',
            content: Array.from(
              { length: 25 },
              (_, index) => `export const GENERATED_LINE_${index + 1} = true;`
            ).join('\n'),
          })}
          isFinished={true}
        />
      </StatusBar>
    </Card>
  ),
  parameters: certification(
    'GENERATED_LINE_20',
    {
      visible: [
        'Write src/generated.ts',
        'added 25 lines',
        'GENERATED_LINE_1',
        'GENERATED_LINE_20',
        '...+5 lines (ctrl+o to toggle)',
      ],
      hidden: ['GENERATED_LINE_21', 'GENERATED_LINE_25'],
      ordered: [
        'Write src/generated.ts',
        'GENERATED_LINE_1',
        'GENERATED_LINE_20',
        '...+5 lines',
      ],
    },
    ['created', 'long-diff-collapsed']
  ),
};
