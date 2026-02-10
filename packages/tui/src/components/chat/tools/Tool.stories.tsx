import React from 'react';
import { Box } from 'ink';
import { Tool } from './Tool.js';

const meta = {
  component: Tool,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'BasicTool',
      'UsingState',
      'UsedState',
      'WithLocations',
      'WithMultipleLocations',
      'WithOutput',
      'WithError',
      'WithLongOutput',
      'MultipleTool',
      'Standalone',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// Basic tool display with status bar
export const BasicTool = {
  args: {
    name: 'web_search',
    status: 'success',
    isFinished: true,
  },
};

// Using state (in progress) with status bar
export const UsingState = {
  args: {
    name: 'grep_search',
    status: 'active',
    isFinished: false,
  },
};

// Used state (finished) with status bar
export const UsedState = {
  args: {
    name: 'list_directory',
    status: 'success',
    isFinished: true,
  },
};

// With file locations and status bar
export const WithLocations = {
  render: () => (
    <Tool
      name="grep_search"
      status="success"
      isFinished={true}
      locations={[{ path: 'src/components/Button.tsx', line: 42 }]}
    />
  ),
};

// With multiple file locations and status bar
export const WithMultipleLocations = {
  render: () => (
    <Tool
      name="find_references"
      status="success"
      isFinished={true}
      locations={[
        { path: 'src/index.ts', line: 10 },
        { path: 'src/utils/helpers.ts', line: 25 },
        { path: 'src/components/App.tsx', line: 5 },
        { path: 'src/types/index.ts', line: 1 },
        { path: 'src/hooks/useAuth.ts', line: 15 },
      ]}
    />
  ),
};

// With successful output and status bar
export const WithOutput = {
  render: () => (
    <Tool
      name="web_search"
      status="success"
      isFinished={true}
      result={{
        status: 'success',
        output: {
          text: 'Found 3 results:\n1. React documentation\n2. TypeScript handbook\n3. Node.js guides',
        },
      }}
    />
  ),
};

// With error and status bar
export const WithError = {
  render: () => (
    <Tool
      name="api_call"
      status="error"
      isFinished={true}
      errorMessage="Connection timeout: unable to reach server"
    />
  ),
};

// With long output (collapsible) and status bar
export const WithLongOutput = {
  render: () => (
    <Tool
      name="grep_search"
      status="success"
      isFinished={true}
      locations={[{ path: 'src/utils.ts' }]}
      result={{
        status: 'success',
        output: {
          text: `Match 1: src/utils.ts:10 - export function helper()
Match 2: src/utils.ts:25 - export function format()
Match 3: src/utils.ts:40 - export function parse()
Match 4: src/utils.ts:55 - export function validate()
Match 5: src/utils.ts:70 - export function transform()
Match 6: src/utils.ts:85 - export function serialize()
Match 7: src/utils.ts:100 - export function deserialize()`,
        },
      }}
    />
  ),
};

// Multiple tools in sequence with status bars
export const MultipleTool = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Tool name="web_search" status="success" isFinished={true} />
      <Tool
        name="grep_search"
        status="success"
        isFinished={true}
        locations={[{ path: 'src/index.ts', line: 5 }]}
      />
      <Tool name="think" status="active" isFinished={false} />
    </Box>
  ),
};

// Standalone without StatusBar wrapper
export const Standalone = {
  args: {
    name: 'custom_tool',
    isFinished: true,
    noStatusBar: true,
  },
};
