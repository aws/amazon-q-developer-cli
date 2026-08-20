import React from 'react';
import { Box } from './../../../renderer.js';
import { Shell } from './Shell.js';
import { Card } from '../../ui/card/Card.js';
import { Text } from '../../ui/text/Text.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';
import { Kiro } from '../../../kiro.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';

const meta = {
  component: Shell,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      success: { label: 'Completed command with output' },
      error: { label: 'Completed command error' },
      loading: { label: 'Command is executing before output arrives' },
      'running-tail': { label: 'Running command shows latest output lines' },
      'finished-head': {
        label: 'Completed command shows earliest output lines',
      },
      'timeout-error': { label: 'Command timeout error' },
      'timeout-detail': {
        label: 'Command timeout retains backend duration detail',
        gapType: 'product-limitation',
        description:
          'The renderer replaces the backend timeout detail with a generic message.',
      },
      'nonzero-exit': {
        label: 'Non-zero exit status indicator',
        gapType: 'visual-baseline-required',
        description:
          'The non-zero distinction is color-only, which text assertions cannot prove.',
      },
      'static-summary': {
        label: 'Past command retains completed output',
        gapType: 'product-limitation',
        description:
          'Legacy static Shell rendering currently hides completed output.',
      },
      arguments: { label: 'Additional command metadata' },
      expanded: {
        label: 'Expanded long command output',
        gapType: 'integration-only',
        description:
          'Expansion is owned by the global tool-output controller, not Shell.',
      },
    },
    storyOrder: [
      'Loading',
      'ListDirectory',
      'Error',
      'Timeout',
      'NonZeroExit',
      'StaticHistory',
      'WithArguments',
      'RunningShowsTail',
      'FinishedShowsHead',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

const manyLines = Array.from({ length: 12 }, (_, i) => `line ${i + 1}: output`);

const viewport = { columns: 120, rows: 20 };

function RunningShellStory(): React.ReactElement {
  const [store] = React.useState(() => {
    const appStore = createAppStore({
      kiro: new Kiro(),
      agentEngine: 'kas',
    });
    appStore.setState({
      liveOutputs: new Map([['running-shell', [manyLines]]]),
    });
    return appStore;
  });
  return (
    <AppStoreContext.Provider value={store}>
      <Box flexDirection="column" gap={1}>
        <Text>
          During execution: last 5 lines shown, &quot;...+N lines above&quot;
          hint
        </Text>
        <Card active={true}>
          <Shell
            name="Bash"
            command="npm install"
            toolCallId="running-shell"
            status="loading"
            isFinished={false}
          />
        </Card>
      </Box>
    </AppStoreContext.Provider>
  );
}

export const Loading = {
  render: () => (
    <Card active={true}>
      <Shell
        name="Bash"
        command="npm run build"
        status="executing"
        isFinished={false}
      />
    </Card>
  ),
  parameters: certifyVisualStory(
    'npm run build',
    {
      visible: ['Bash npm run build'],
      hidden: ['undefined', 'Build completed'],
    },
    ['loading'],
    viewport
  ),
};

export const ListDirectory = {
  render: () => (
    <Card active={true}>
      <Shell
        name="Bash"
        command="ls -la"
        status="success"
        isFinished
        result={{
          status: 'success',
          output: [
            'drwxr-xr-x 12 user staff 384 Jan 14 10:30 .',
            '-rw-r--r-- 1 user staff 1024 Jan 14 09:15 README.md',
            'drwxr-xr-x 5 user staff 160 Jan 12 14:30 src',
          ].join('\n'),
        }}
      />
    </Card>
  ),
  parameters: certifyVisualStory(
    'README.md',
    {
      visible: ['Bash ls -la', 'README.md', 'src'],
      ordered: ['Bash ls -la', 'README.md', 'src'],
    },
    ['success'],
    viewport
  ),
};

export const Error = {
  render: () => (
    <Card active={true}>
      <Shell
        name="Bash"
        command="ls /missing"
        status="error"
        isFinished
        result={{
          status: 'error',
          error: 'Could not list any files or folders in this directory',
        }}
      />
    </Card>
  ),
  parameters: certifyVisualStory(
    'Could not list any files',
    {
      visible: [
        'Bash ls /missing',
        'Could not list any files or folders in this directory',
      ],
    },
    ['error'],
    viewport
  ),
};

export const Timeout = {
  render: () => (
    <Card active={true}>
      <Shell
        name="Bash"
        command="sleep 60"
        status="error"
        isFinished
        result={{
          status: 'error',
          error: 'Process timed out after 30 seconds',
        }}
      />
    </Card>
  ),
  parameters: certifyVisualStory(
    'Command timed out',
    {
      visible: ['Bash sleep 60', 'Command timed out'],
      hidden: ['undefined'],
    },
    ['timeout-error'],
    viewport
  ),
};

export const NonZeroExit = {
  render: () => (
    <Card active={true}>
      <Shell
        name="Bash"
        command="npm test"
        isFinished
        result={{
          status: 'success',
          output: {
            stderr: 'FAIL src/parser.test.ts',
            exit_status: 1,
          },
        }}
      />
    </Card>
  ),
  parameters: certifyVisualStory(
    'FAIL src/parser.test.ts',
    {
      visible: ['Bash npm test', 'FAIL src/parser.test.ts'],
      hidden: ['undefined'],
    },
    [],
    viewport
  ),
};

export const StaticHistory = {
  render: () => (
    <Card active={true}>
      <Shell
        name="Bash"
        command="git status --short"
        status="success"
        isFinished
        isStatic
        result={{
          status: 'success',
          output: 'M packages/tui/src/index.tsx',
        }}
      />
    </Card>
  ),
  parameters: certifyVisualStory(
    'git status --short',
    {
      visible: ['Bash git status --short'],
      hidden: ['undefined'],
    },
    [],
    viewport
  ),
};

export const WithArguments = {
  render: () => (
    <Card active={true}>
      <Shell
        name="Bash"
        command="cargo test"
        status="success"
        isFinished
        content={JSON.stringify({
          command: 'cargo test',
          timeout_ms: 120000,
          cwd: '/workspace/kiro-cli',
        })}
        result={{ status: 'success', output: 'test result: ok' }}
      />
    </Card>
  ),
  parameters: certifyVisualStory(
    'timeout_ms=120000',
    {
      visible: [
        'Bash cargo test',
        'timeout_ms=120000',
        'cwd=/workspace/kiro-cli',
        'test result: ok',
      ],
      ordered: [
        'Bash cargo test',
        'timeout_ms=120000',
        'cwd=/workspace/kiro-cli',
        'test result: ok',
      ],
    },
    ['arguments'],
    viewport
  ),
};

export const RunningShowsTail = {
  render: RunningShellStory,
  parameters: certifyVisualStory(
    'line 12: output',
    {
      visible: ['...+7 lines above', 'line 8: output', 'line 12: output'],
      hidden: ['line 1: output', 'line 7: output'],
    },
    ['running-tail'],
    viewport
  ),
};

export const FinishedShowsHead = {
  render: () => (
    <Box flexDirection="column" gap={1}>
      <Text>After completion: first 5 lines shown, expand hint at bottom</Text>
      <Card active={true}>
        <Shell
          name="Bash"
          command="npm install"
          status="success"
          isFinished={true}
          result={{
            status: 'success',
            output: manyLines.join('\n'),
          }}
        />
      </Card>
    </Box>
  ),
  parameters: certifyVisualStory(
    'line 1: output',
    {
      visible: ['line 1: output', 'line 5: output', 'ctrl+o to toggle'],
      hidden: ['line 6: output', 'line 12: output'],
    },
    ['finished-head'],
    viewport
  ),
};
