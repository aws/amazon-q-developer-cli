import React from 'react';
import { Box, Text } from 'ink';
import { PromptBar, type PromptBarHeader } from './PromptBar.js';
import { ContextBar } from './ContextBar.js';
import { Chip, ChipColor } from '../../ui/chip/index.js';
import { SnackBar } from './SnackBar.js';
import { Menu } from '../../ui/menu/Menu.js';

// Pre-create header elements to avoid type inference issues
const contextBarBasic = React.createElement(ContextBar, {
  children: [
    React.createElement(Chip, {
      key: 'workspace',
      value: '~/developer/my-project',
      color: ChipColor.BRAND,
    }),
    React.createElement(Chip, {
      key: 'git',
      value: 'main',
      color: ChipColor.PRIMARY,
      prefix: 'git:',
      wrap: true,
    }),
    React.createElement(Chip, {
      key: 'model',
      value: 'claude-3.5-sonnet',
      color: ChipColor.PRIMARY,
    }),
  ],
}) as PromptBarHeader;

const contextBarProcessing = React.createElement(ContextBar, {
  children: [
    React.createElement(Chip, {
      key: 'workspace',
      value: '~/developer/my-project',
      color: ChipColor.BRAND,
    }),
    React.createElement(Chip, {
      key: 'git',
      value: 'feature/new-component',
      color: ChipColor.PRIMARY,
      prefix: 'git:',
      wrap: true,
    }),
    React.createElement(Chip, { key: 'model', value: 'gpt-4', color: ChipColor.PRIMARY }),
  ],
}) as PromptBarHeader;

const contextBarNoGit = React.createElement(ContextBar, {
  children: [
    React.createElement(Chip, {
      key: 'workspace',
      value: '~/developer/my-project',
      color: ChipColor.BRAND,
    }),
    React.createElement(Chip, {
      key: 'model',
      value: 'claude-3.5-sonnet',
      color: ChipColor.PRIMARY,
    }),
  ],
}) as PromptBarHeader;

const contextBarMinimal = React.createElement(ContextBar, {
  children: [React.createElement(Chip, { key: 'model', value: 'gpt-4', color: ChipColor.PRIMARY })],
}) as PromptBarHeader;

const snackBarExample = React.createElement(SnackBar, {
  title: 'Apply Changes',
  actions: [
    { label: 'Apply', key: 'apply' },
    { label: 'Cancel', key: 'cancel' },
  ],
}) as PromptBarHeader;

const meta = {
  component: PromptBar,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Basic', 'Processing', 'NoGit', 'Minimal', 'WithSnackBar', 'WithValue', 'WithMenu', 'WithApproval', 'LongPlaceholder'],
  },
  tags: ['autodocs'],
};

export default meta;

export const Basic = {
  parameters: {
    docs: {
      storyDescription: 'Basic prompt bar with status information',
    },
  },
  args: {
    header: contextBarBasic,
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    placeholder: 'ask a question, or describe a task ↵',
  },
};

export const Processing = {
  parameters: {
    docs: {
      storyDescription: 'Prompt bar in processing state',
    },
  },
  args: {
    header: contextBarProcessing,
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: true,
    placeholder: 'Processing your request...',
  },
};

export const NoGit = {
  parameters: {
    docs: {
      storyDescription: 'Prompt bar without git information',
    },
  },
  args: {
    header: contextBarNoGit,
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    placeholder: 'ask a question, or describe a task ↵',
  },
};

export const Minimal = {
  parameters: {
    docs: {
      storyDescription: 'Minimal prompt bar with only model information',
    },
  },
  args: {
    header: contextBarMinimal,
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    placeholder: 'ask a question, or describe a task ↵',
  },
};

export const WithSnackBar = {
  parameters: {
    docs: {
      storyDescription: 'Prompt bar with snack bar header for actions',
    },
  },
  args: {
    header: snackBarExample,
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    placeholder: 'ask a question, or describe a task ↵',
  },
};

export const WithValue = {
  parameters: {
    docs: {
      storyDescription: 'Prompt bar with controlled input value',
    },
  },
  args: {
    header: contextBarBasic,
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    value: '/model',
    clearOnSubmit: false,
  },
};

export const WithMenu = {
  parameters: {
    docs: {
      storyDescription: 'Prompt bar with menu children (footer area)',
    },
  },
  render: () => (
    <PromptBar
      header={contextBarBasic}
      onSubmit={(cmd) => console.log('Command:', cmd)}
      isProcessing={false}
      value="/mo"
    >
      <Menu
        items={[
          { label: 'model', description: 'Select or list available models' },
          { label: 'mcp', description: 'Manage MCP servers' },
        ]}
        prefix="/"
        onSelect={(item) => console.log('Selected:', item)}
        onEscape={() => console.log('Escape')}
      />
    </PromptBar>
  ),
};

export const WithApproval = {
  parameters: {
    docs: {
      storyDescription: 'Prompt bar with approval snackbar in footer',
    },
  },
  render: () => (
    <PromptBar
      header={contextBarBasic}
      onSubmit={(cmd) => console.log('Command:', cmd)}
      isProcessing={true}
    >
      <SnackBar
        title="Tool requires approval"
        actions={[
          { key: 'y', label: 'Yes' },
          { key: 'n', label: 'No' },
          { key: 't', label: 'Trust' },
        ]}
      />
    </PromptBar>
  ),
};

export const LongPlaceholder = {
  parameters: {
    docs: {
      storyDescription: 'Prompt bar with long placeholder text',
    },
  },
  args: {
    header: contextBarBasic,
    onSubmit: (command: string) => console.log('Command:', command),
    isProcessing: false,
    placeholder:
      'ask a detailed question about your codebase, request code changes, or describe a complex task you need help with ↵',
  },
};
