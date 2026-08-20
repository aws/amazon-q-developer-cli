import { Read } from './Read.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 120, rows: 24 };
const outputEnvironment = { KIRO_LITE_ROLLOUT_ENABLED: '1' };

const meta = {
  component: Read,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: { label: 'Single-file read in progress' },
      'header-only': { label: 'Completed read in the default cohort' },
      'line-range': { label: 'Read target includes its requested line range' },
      'source-body': { label: 'Source body with line numbers' },
      'collapsed-source': {
        label: 'Collapsed long source body shows its tail',
      },
      'wrapped-source': { label: 'Long source line wraps at narrow width' },
      'multiple-files': { label: 'Multiple file targets' },
      'collapsed-files': { label: 'Collapsed long file target list' },
      'windows-path': {
        label: 'Windows multi-file targets display their basenames',
      },
      directory: {
        label: 'Directory output uses parsed entry names',
        gapType: 'product-limitation',
        description:
          'Read sends Rust long-format directory rows directly to ToolOutput instead of parsing them.',
      },
      'directory-body': { label: 'Directory result body is displayed' },
      mixed: {
        label: 'Mixed output preserves typed formatting per operation',
        gapType: 'product-limitation',
        description:
          'Directory rows remain in Rust long format while source rows receive source formatting.',
      },
      'mixed-body': {
        label: 'Mixed directory and source bodies are displayed',
      },
      empty: { label: 'Successful read with no output' },
      'static-summary': { label: 'Past read suppresses transient output' },
      error: {
        label: 'Read error',
        gapType: 'integration-only',
        description:
          'ToolUseMessage routes read failures through the shared error renderer before Read.',
      },
      expanded: {
        label: 'Expanded long file or source output',
        gapType: 'integration-only',
        description:
          'Expansion is owned by the global tool-output controller, not Read.',
      },
      'character-capped': {
        label: 'Source lines respect display character cap',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output character caps.',
      },
      'multi-file-association': {
        label: 'Each multi-file source body retains its file identity',
        gapType: 'product-limitation',
        description:
          'Combined source bodies currently render without per-file headings.',
      },
      'image-semantics': {
        label: 'Unified image operations use image-specific labels',
        gapType: 'product-limitation',
        description:
          'Image operations routed through Read currently use file terminology.',
      },
    },
    storyOrder: [
      'Loading',
      'HeaderOnly',
      'SourceBody',
      'LongSourceBody',
      'WrappedSourceBody',
      'MultipleFiles',
      'WindowsPaths',
      'ManyFiles',
      'Directory',
      'MixedDirectoryAndSource',
      'EmptyOutput',
      'StaticHistory',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Loading = {
  args: {
    content: JSON.stringify({
      path: 'src/components/Button.tsx',
      offset: 9,
      limit: 20,
    }),
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'Button.tsx',
    {
      visible: ['Read src/components/Button.tsx (L10-29)'],
      hidden: ['output:', 'const Button'],
    },
    ['loading', 'line-range'],
    viewport
  ),
};

export const HeaderOnly = {
  args: {
    content: JSON.stringify({ path: 'src/components/Button.tsx' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{ Text: "export const Button = () => 'BUTTON_BODY';" }],
      },
    },
  },
  parameters: certifyVisualStory(
    'Button.tsx',
    {
      visible: ['Read src/components/Button.tsx'],
      hidden: ['output:', 'BUTTON_BODY'],
    },
    ['header-only'],
    viewport
  ),
};

export const SourceBody = {
  args: {
    content: JSON.stringify({
      path: 'src/components/Button.tsx',
      offset: 9,
      limit: 3,
    }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: [
              'export function Button() {',
              '  return <Box>BUTTON_BODY</Box>;',
              '}',
            ].join('\n'),
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'BUTTON_BODY',
    {
      visible: [
        'Read src/components/Button.tsx (L10-12)',
        'output:',
        '10  export function Button()',
        '11    return <Box>BUTTON_BODY</Box>;',
        '12  }',
      ],
      ordered: [
        'Read src/components/Button.tsx (L10-12)',
        'output:',
        '10  export function Button()',
        '11    return <Box>BUTTON_BODY</Box>;',
        '12  }',
      ],
    },
    ['line-range', 'source-body'],
    viewport,
    outputEnvironment
  ),
};

export const LongSourceBody = {
  args: {
    content: JSON.stringify({ path: 'src/long.ts' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: Array.from(
              { length: 25 },
              (_, index) => `READ_LINE_${index + 1}`
            ).join('\n'),
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'READ_LINE_25',
    {
      visible: [
        'Read src/long.ts',
        'lines above (ctrl+o to toggle)',
        'READ_LINE_25',
      ],
      hidden: ['READ_LINE_1\n', 'READ_LINE_5\n'],
      ordered: ['Read src/long.ts', 'lines above', 'READ_LINE_25'],
    },
    ['source-body', 'collapsed-source'],
    viewport,
    outputEnvironment
  ),
};

export const WrappedSourceBody = {
  args: {
    content: JSON.stringify({ path: 'src/wrapped.ts', offset: 4, limit: 1 }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: 'const WRAPPED_SOURCE_MARKER = "a deliberately long source value";',
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'WRAPPED_SOURCE_MARKER',
    {
      visible: [
        'Read src/wrapped.ts (L5-5)',
        '5  const WRAPPED_SOURCE_MARKER',
        'deliberately long source value',
      ],
      ordered: [
        'Read src/wrapped.ts (L5-5)',
        '5  const WRAPPED_SOURCE_MARKER',
        'deliberately long source value',
      ],
    },
    ['line-range', 'source-body', 'wrapped-source'],
    { columns: 54, rows: 14 },
    outputEnvironment
  ),
};

export const MultipleFiles = {
  args: {
    content: JSON.stringify({
      paths: ['src/index.ts', 'src/app.ts', 'src/config.ts'],
    }),
    isFinished: true,
  },
  parameters: certifyVisualStory(
    '3 files',
    {
      visible: ['Read (3 files)', 'index.ts, app.ts, config.ts'],
      ordered: ['Read (3 files)', 'index.ts, app.ts, config.ts'],
    },
    ['multiple-files'],
    viewport
  ),
};

export const WindowsPaths = {
  args: {
    content: JSON.stringify({
      paths: ['C:\\workspace\\src\\App.tsx', 'C:\\workspace\\src\\Store.ts'],
    }),
    isFinished: true,
  },
  parameters: certifyVisualStory(
    '2 files',
    {
      visible: ['Read (2 files)', 'App.tsx, Store.ts'],
      hidden: ['C:\\workspace\\src\\App.tsx', 'C:\\workspace\\src\\Store.ts'],
      ordered: ['Read (2 files)', 'App.tsx, Store.ts'],
    },
    ['multiple-files', 'windows-path'],
    viewport
  ),
};

export const ManyFiles = {
  args: {
    content: JSON.stringify({
      paths: [
        'src/one.ts',
        'src/two.ts',
        'src/three.ts',
        'src/four.ts',
        'src/five.ts',
        'src/six.ts',
        'src/seven.ts',
      ],
    }),
    isFinished: true,
  },
  parameters: certifyVisualStory(
    '7 files',
    {
      visible: [
        'Read (7 files)',
        'one.ts, two.ts, three.ts, four.ts, five.ts',
        '...+2 files (ctrl+o to toggle)',
      ],
      hidden: ['six.ts', 'seven.ts'],
    },
    ['multiple-files', 'collapsed-files'],
    viewport
  ),
};

export const Directory = {
  args: {
    content: JSON.stringify({
      operations: [{ path: 'src/components', mode: 'Directory' }],
    }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: [
              'User id: 501',
              '-rw-r--r-- 1 501 20 4096 Feb 17 21:01 /workspace/src/components/Button.tsx',
              '-rw-r--r-- 1 501 20 2048 Feb 17 20:55 /workspace/src/components/Card.tsx',
              '-rw-r--r-- 1 501 20 8192 Feb 17 20:40 /workspace/src/components/Prompt.tsx',
            ].join('\n'),
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'Prompt.tsx',
    {
      visible: [
        'Read src/components',
        'output:',
        'Button.tsx',
        'Card.tsx',
        'Prompt.tsx',
      ],
      ordered: ['Read src/components', 'Button.tsx', 'Card.tsx', 'Prompt.tsx'],
    },
    ['directory-body'],
    viewport,
    outputEnvironment
  ),
};

export const MixedDirectoryAndSource = {
  args: {
    content: JSON.stringify({
      operations: [
        { path: 'src/components', mode: 'Directory' },
        { path: 'src/index.ts', mode: 'Line', offset: 4, limit: 1 },
      ],
    }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: [
              'User id: 501',
              '-rw-r--r-- 1 501 20 4096 Feb 17 21:01 /workspace/src/components/Button.tsx',
              '-rw-r--r-- 1 501 20 2048 Feb 17 20:55 /workspace/src/components/Card.tsx',
            ].join('\n'),
          },
          { Text: 'export const MIXED_SOURCE = true;' },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'MIXED_SOURCE',
    {
      visible: [
        'Read (2 files)',
        'Button.tsx',
        'Card.tsx',
        '5  export const MIXED_SOURCE = true;',
      ],
      ordered: [
        'Read (2 files)',
        'Button.tsx',
        'Card.tsx',
        '5  export const MIXED_SOURCE = true;',
      ],
    },
    ['multiple-files', 'mixed-body'],
    viewport,
    outputEnvironment
  ),
};

export const EmptyOutput = {
  args: {
    content: JSON.stringify({ path: 'src/empty.ts' }),
    isFinished: true,
    result: {
      status: 'success',
      output: { items: [{ Text: '' }] },
    },
  },
  parameters: certifyVisualStory(
    '(no output)',
    {
      visible: ['Read src/empty.ts', 'output:', '(no output)'],
    },
    ['empty'],
    viewport,
    outputEnvironment
  ),
};

export const StaticHistory = {
  args: {
    content: JSON.stringify({ path: 'src/history.ts' }),
    isFinished: true,
    isStatic: true,
    result: {
      status: 'success',
      output: { items: [{ Text: 'STATIC_READ_BODY' }] },
    },
  },
  parameters: certifyVisualStory(
    'history.ts',
    {
      visible: ['Read src/history.ts'],
      hidden: ['output:', 'STATIC_READ_BODY', 'ctrl+o'],
    },
    ['static-summary'],
    viewport,
    outputEnvironment
  ),
};
