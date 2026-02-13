import { Glob } from './Glob.js';

const meta = {
  component: Glob,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Globbing',
      'NoFiles',
      'FewFiles',
      'ManyFiles',
      'Truncated',
      'Error',
      'Standalone',
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
};
