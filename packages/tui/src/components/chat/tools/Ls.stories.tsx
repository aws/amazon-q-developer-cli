import { Ls } from './Ls.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 120, rows: 16 };

function listing(...paths: string[]): string {
  return [
    'User id: 501',
    ...paths.map((path) => `-rw-r--r-- 1 501 20 1024 Jan 15 10:30 ${path}`),
  ].join('\n');
}

const meta = {
  component: Ls,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: {
        label: 'Directory listing in-progress indicator',
        gapType: 'visual-baseline-required',
        description:
          'Loading differs only through animated status styling, which text assertions cannot prove.',
      },
      empty: { label: 'Empty directory' },
      singular: { label: 'Directory with one entry' },
      listed: { label: 'Short directory listing' },
      'resolved-path': {
        label: 'Relative input resolves to the returned absolute path',
      },
      'space-in-name': { label: 'Entry name containing spaces' },
      collapsed: { label: 'Collapsed long directory listing' },
      static: { label: 'Past listing uses a hidden-entry suffix' },
      error: { label: 'Directory listing error' },
      standalone: { label: 'Listing embedded without status chrome' },
      expanded: {
        label: 'Expanded directory listing',
        gapType: 'integration-only',
        description:
          'Expansion is owned by the global tool-output controller, not Ls.',
      },
      'truncation-warning': {
        label: 'Backend truncation warning remains visible',
        gapType: 'product-limitation',
        description:
          'The parser currently removes directory truncation notices.',
      },
      'output-filtered': {
        label: 'Output hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
    },
    storyOrder: [
      'Listing',
      'Empty',
      'SingleEntry',
      'ListedResolvedPath',
      'ManyEntries',
      'StaticSummary',
      'Error',
      'Standalone',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Listing = {
  args: {
    content: JSON.stringify({ path: 'src/components' }),
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'Ls src/components',
    {
      visible: ['Ls src/components'],
      hidden: ['entry', 'ctrl+o'],
    },
    [],
    viewport
  ),
};

export const Empty = {
  args: {
    content: JSON.stringify({ path: '/workspace/empty' }),
    isFinished: true,
    result: {
      status: 'success',
      output: { items: [{ Text: 'User id: 501' }] },
    },
  },
  parameters: certifyVisualStory(
    'Ls /workspace/empty',
    {
      visible: ['Ls /workspace/empty'],
      hidden: ['entry', 'ctrl+o', 'User id:'],
    },
    ['empty'],
    viewport
  ),
};

export const SingleEntry = {
  args: {
    content: JSON.stringify({ path: '/workspace/config' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{ Text: listing('/workspace/config/settings.json') }],
      },
    },
  },
  parameters: certifyVisualStory(
    'settings.json',
    {
      visible: ['Ls /workspace/config', '1 entry', 'settings.json'],
      hidden: ['User id:', 'ctrl+o'],
    },
    ['singular'],
    viewport
  ),
};

export const ListedResolvedPath = {
  args: {
    content: JSON.stringify({ path: '.' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: listing(
              '/home/user/Documents/game.py',
              '/home/user/Documents/project notes.md'
            ),
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'project notes.md',
    {
      visible: [
        'Ls /home/user/Documents',
        '2 entries',
        'game.py, project notes.md',
      ],
      hidden: ['User id:', '10:30'],
    },
    ['listed', 'resolved-path', 'space-in-name'],
    viewport
  ),
};

const manyPaths = [
  '/workspace/src/components',
  '/workspace/src/hooks',
  '/workspace/src/utils',
  '/workspace/src/stores',
  '/workspace/src/types',
  '/workspace/src/index.tsx',
  '/workspace/src/App.tsx',
  '/workspace/src/kiro.ts',
];

export const ManyEntries = {
  args: {
    content: JSON.stringify({ path: '/workspace/src', depth: 1 }),
    isFinished: true,
    result: {
      status: 'success',
      output: { items: [{ Text: listing(...manyPaths) }] },
    },
  },
  parameters: certifyVisualStory(
    '8 entries',
    {
      visible: [
        'Ls /workspace/src',
        'depth=1',
        '8 entries',
        'components, hooks, utils, stores, types',
        '...+3 entries (ctrl+o to toggle)',
      ],
      hidden: ['index.tsx', 'kiro.ts'],
    },
    ['collapsed'],
    viewport
  ),
};

export const StaticSummary = {
  args: {
    content: JSON.stringify({ path: '/workspace/src' }),
    isFinished: true,
    isStatic: true,
    result: {
      status: 'success',
      output: { items: [{ Text: listing(...manyPaths) }] },
    },
  },
  parameters: certifyVisualStory(
    '+3 more',
    {
      visible: ['8 entries', 'components, hooks, utils, stores, types +3 more'],
      hidden: ['index.tsx', 'kiro.ts', 'ctrl+o'],
    },
    ['static'],
    viewport
  ),
};

export const Error = {
  args: {
    content: JSON.stringify({ path: '/nonexistent/dir' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'Directory not found: /nonexistent/dir',
    },
  },
  parameters: certifyVisualStory(
    'Directory not found',
    {
      visible: ['Ls /nonexistent/dir', 'Directory not found: /nonexistent/dir'],
    },
    ['error'],
    viewport
  ),
};

export const Standalone = {
  args: {
    content: JSON.stringify({ path: '/workspace/packages/tui' }),
    noStatusBar: true,
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: listing(
              '/workspace/packages/tui/src',
              '/workspace/packages/tui/package.json',
              '/workspace/packages/tui/tsconfig.json'
            ),
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'package.json',
    {
      visible: [
        'Ls /workspace/packages/tui',
        '3 entries',
        'src, package.json, tsconfig.json',
      ],
    },
    ['listed', 'standalone'],
    viewport
  ),
};
