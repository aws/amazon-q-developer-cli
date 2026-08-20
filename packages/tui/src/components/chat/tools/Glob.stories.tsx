import { Glob } from './Glob.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 120, rows: 18 };

const meta = {
  component: Glob,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: { label: 'File search in progress' },
      empty: { label: 'File search completed with no matches' },
      singular: { label: 'One matching file' },
      few: { label: 'Short matching file list' },
      collapsed: { label: 'Collapsed long file list' },
      truncated: { label: 'Backend reports incomplete results' },
      error: { label: 'File search error' },
      standalone: { label: 'File search embedded without status chrome' },
      'kas-text': { label: 'KAS plain-text result envelope' },
      static: { label: 'Past search uses a hidden-file suffix' },
      'output-tree': { label: 'Rollout output-tree presentation' },
      'windows-path': { label: 'Windows result path displays its basename' },
      expanded: {
        label: 'Expanded file list',
        gapType: 'integration-only',
        description:
          'Expansion is owned by the global tool-output controller, not Glob.',
      },
      'kas-incomplete-count': {
        label: 'KAS incomplete result preserves the backend total',
        gapType: 'product-limitation',
        description:
          'The text envelope exposes returned paths but not the full match count.',
      },
      'output-filtered': {
        label: 'Output hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
    },
    storyOrder: [
      'Globbing',
      'NoFiles',
      'OneFile',
      'WindowsPath',
      'FewFiles',
      'ManyFiles',
      'Truncated',
      'Error',
      'Standalone',
      'KasTextResult',
      'StaticSummary',
      'OutputTree',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// Globbing state (in progress)
export const Globbing = {
  args: {
    name: 'Globbing',
    content: JSON.stringify({ pattern: '**/*.tsx' }),
    status: 'active',
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'Glob "**/*.tsx"',
    {
      visible: ['Glob "**/*.tsx"'],
      hidden: ['files', 'output:'],
    },
    ['loading'],
    viewport
  ),
};

// No files found
export const NoFiles = {
  args: {
    name: 'Globbed',
    content: JSON.stringify({ pattern: '**/*.xyz' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              filePaths: [],
              totalFiles: 0,
              truncated: false,
              message: 'No files found matching pattern: **/*.xyz',
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'No files found',
    {
      visible: ['Glob "**/*.xyz"', 'No files found matching pattern: **/*.xyz'],
      hidden: ['1 file', 'You searched for'],
    },
    ['empty'],
    viewport
  ),
};

export const OneFile = {
  args: {
    content: JSON.stringify({ pattern: 'src/config.ts' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        filePaths: ['src/config.ts'],
        totalFiles: 1,
        truncated: false,
      },
    },
  },
  parameters: certifyVisualStory(
    'config.ts',
    {
      visible: ['Glob "src/config.ts"', '1 file', 'config.ts'],
      hidden: ['ctrl+o'],
    },
    ['singular'],
    viewport
  ),
};

export const WindowsPath = {
  args: {
    content: JSON.stringify({ pattern: '**/*.tsx' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        filePaths: ['C:\\workspace\\src\\App.tsx'],
        totalFiles: 1,
        truncated: false,
      },
    },
  },
  parameters: certifyVisualStory(
    'App.tsx',
    {
      visible: ['Glob "**/*.tsx"', '1 file', 'App.tsx'],
      hidden: ['C:\\workspace\\src\\App.tsx'],
    },
    ['singular', 'windows-path'],
    viewport
  ),
};

// Few files found (no truncation needed)
export const FewFiles = {
  args: {
    name: 'Globbed',
    content: JSON.stringify({ pattern: 'src/**/*.test.ts' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              filePaths: [
                'src/utils/helpers.test.ts',
                'src/utils/math.test.ts',
                'src/components/Button.test.ts',
              ],
              totalFiles: 3,
              truncated: false,
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'helpers.test.ts',
    {
      visible: [
        'Glob "src/**/*.test.ts"',
        '3 files',
        'helpers.test.ts, math.test.ts, Button.test.ts',
      ],
      hidden: ['ctrl+o'],
    },
    ['few'],
    viewport
  ),
};

// Many files found
export const ManyFiles = {
  args: {
    name: 'Globbed',
    content: JSON.stringify({ pattern: '**/*.tsx' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              filePaths: [
                'src/components/Button.tsx',
                'src/components/Modal.tsx',
                'src/components/Card.tsx',
                'src/components/Form.tsx',
                'src/components/Input.tsx',
                'src/components/Select.tsx',
                'src/components/Table.tsx',
                'src/components/Tabs.tsx',
                'src/pages/Home.tsx',
                'src/pages/About.tsx',
              ],
              totalFiles: 10,
              truncated: false,
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    '10 files',
    {
      visible: [
        '10 files',
        'Button.tsx, Modal.tsx, Card.tsx',
        '...+7 files (ctrl+o to toggle)',
      ],
      hidden: ['Form.tsx', 'About.tsx'],
    },
    ['collapsed'],
    viewport
  ),
};

// Truncated results
export const Truncated = {
  args: {
    name: 'Globbed',
    content: JSON.stringify({ pattern: '**/*.ts' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              filePaths: [
                'src/index.ts',
                'src/types.ts',
                'src/utils/helpers.ts',
                'src/utils/math.ts',
                'src/utils/string.ts',
                'src/api/client.ts',
                'src/api/types.ts',
              ],
              totalFiles: 150,
              truncated: true,
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'showing first results',
    {
      visible: [
        '150 files (showing first results)',
        'index.ts, types.ts, helpers.ts',
        '...+4 files (ctrl+o to toggle)',
      ],
      hidden: ['client.ts'],
    },
    ['collapsed', 'truncated'],
    viewport
  ),
};

// Error state
export const Error = {
  args: {
    name: 'Globbed',
    content: JSON.stringify({ pattern: '**/*.ts', path: '/nonexistent' }),
    status: 'error',
    isFinished: true,
    result: {
      status: 'error',
      error: 'Path does not exist: /nonexistent',
    },
  },
  parameters: certifyVisualStory(
    'Path does not exist',
    {
      visible: [
        'Glob "**/*.ts"',
        'path=/nonexistent',
        'Path does not exist: /nonexistent',
      ],
    },
    ['error'],
    viewport
  ),
};

// Standalone without StatusBar wrapper
export const Standalone = {
  args: {
    name: 'Globbed',
    content: JSON.stringify({ pattern: '*.json' }),
    noStatusBar: true,
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              filePaths: ['package.json', 'tsconfig.json'],
              totalFiles: 2,
              truncated: false,
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'package.json',
    {
      visible: ['Glob "*.json"', '2 files', 'package.json, tsconfig.json'],
    },
    ['few', 'standalone'],
    viewport
  ),
};

export const KasTextResult = {
  args: {
    content: JSON.stringify({ query: '*.toml' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        message: [
          'You searched for *.toml and received the following results:',
          '---',
          'Cargo.toml',
          'packages/tui/bunfig.toml',
          '---',
          'Search complete',
        ].join('\n'),
      },
    },
  },
  parameters: certifyVisualStory(
    'bunfig.toml',
    {
      visible: ['Glob "*.toml"', '2 files', 'Cargo.toml, bunfig.toml'],
      hidden: ['You searched for', 'Search complete', 'query=*.toml'],
    },
    ['few', 'kas-text'],
    viewport
  ),
};

export const StaticSummary = {
  args: {
    content: JSON.stringify({ pattern: '**/*.ts' }),
    isFinished: true,
    isStatic: true,
    result: {
      status: 'success',
      output: {
        filePaths: [
          'src/one.ts',
          'src/two.ts',
          'src/three.ts',
          'src/four.ts',
          'src/five.ts',
        ],
        totalFiles: 5,
        truncated: false,
      },
    },
  },
  parameters: certifyVisualStory(
    '+2 more',
    {
      visible: ['5 files', 'one.ts, two.ts, three.ts +2 more'],
      hidden: ['four.ts', 'five.ts', 'ctrl+o'],
    },
    ['static'],
    viewport
  ),
};

export const OutputTree = {
  args: {
    content: JSON.stringify({ pattern: 'src/*.ts' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        filePaths: ['src/index.ts', 'src/config.ts'],
        totalFiles: 2,
        truncated: false,
      },
    },
  },
  parameters: certifyVisualStory(
    'config.ts',
    {
      visible: [
        'Glob "src/*.ts"',
        'output:',
        '2 files',
        'index.ts',
        'config.ts',
      ],
      ordered: [
        'Glob "src/*.ts"',
        'output:',
        '2 files',
        'index.ts',
        'config.ts',
      ],
    },
    ['few', 'output-tree'],
    viewport,
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};
