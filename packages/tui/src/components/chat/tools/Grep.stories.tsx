import { Grep } from './Grep.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 120, rows: 20 };

const meta = {
  component: Grep,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: { label: 'Search in progress' },
      empty: { label: 'Search completed with no matches' },
      'single-file': { label: 'Matches in one file' },
      'multiple-files': { label: 'Matches across files' },
      'match-lines': { label: 'Matched source lines' },
      collapsed: { label: 'Collapsed result preview' },
      truncated: { label: 'Backend reports incomplete results' },
      error: { label: 'Search error' },
      standalone: { label: 'Search embedded without status chrome' },
      'kas-text': { label: 'KAS plain-text result envelope' },
      static: { label: 'Past search displays summary only' },
      'output-tree': { label: 'Rollout output-tree presentation' },
      expanded: {
        label: 'Expanded result list',
        gapType: 'integration-only',
        description:
          'Expansion is owned by the global tool-output controller, not Grep.',
      },
      'output-filtered': {
        label: 'Output hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
    },
    storyOrder: [
      'Grepping',
      'NoMatches',
      'SingleFileMatch',
      'MultipleFileMatches',
      'WithMatchContent',
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

// Grepping state (in progress)
export const Grepping = {
  args: {
    name: 'Grepping',
    content: JSON.stringify({ pattern: 'useState' }),
    status: 'active',
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'Grep "useState"',
    {
      visible: ['Grep "useState"'],
      hidden: ['matches in', 'output:'],
    },
    ['loading'],
    viewport
  ),
};

// No matches found
export const NoMatches = {
  args: {
    name: 'Grepped',
    content: JSON.stringify({ pattern: 'nonexistentPattern' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              numMatches: 0,
              numFiles: 0,
              truncated: false,
              message: 'No matches found for pattern: nonexistentPattern',
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'No matches found',
    {
      visible: [
        'Grep "nonexistentPattern"',
        'No matches found for pattern: nonexistentPattern',
      ],
      hidden: ['1 file', 'You searched for'],
    },
    ['empty'],
    viewport
  ),
};

// Single file with matches
export const SingleFileMatch = {
  args: {
    name: 'Grepped',
    content: JSON.stringify({ pattern: 'useState' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              numMatches: 3,
              numFiles: 1,
              truncated: false,
              results: [
                {
                  file: 'src/components/Button.tsx',
                  count: 3,
                  matches: [
                    '12:const [isOpen, setIsOpen] = useState(false);',
                    '15:const [count, setCount] = useState(0);',
                    '18:const [name, setName] = useState("");',
                  ],
                },
              ],
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'Button.tsx',
    {
      visible: [
        'Grep "useState"',
        '3 matches in 1 file',
        'Button.tsx (3)',
        '12:const [isOpen, setIsOpen] = useState(false);',
      ],
      ordered: [
        '3 matches in 1 file',
        'Button.tsx (3)',
        '12:const [isOpen, setIsOpen] = useState(false);',
      ],
    },
    ['single-file', 'match-lines'],
    viewport
  ),
};

// Multiple files with matches
export const MultipleFileMatches = {
  args: {
    name: 'Grepped',
    content: JSON.stringify({ pattern: 'import.*React' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              numMatches: 15,
              numFiles: 8,
              truncated: false,
              results: [
                {
                  file: 'src/components/Button.tsx',
                  count: 1,
                  matches: ["1:import React from 'react';"],
                },
                {
                  file: 'src/components/Modal.tsx',
                  count: 1,
                  matches: ["1:import React, { useState } from 'react';"],
                },
                {
                  file: 'src/components/Card.tsx',
                  count: 1,
                  matches: ["1:import React from 'react';"],
                },
                {
                  file: 'src/hooks/useTheme.ts',
                  count: 1,
                  matches: ["1:import { useContext } from 'react';"],
                },
                {
                  file: 'src/App.tsx',
                  count: 1,
                  matches: ["1:import React from 'react';"],
                },
              ],
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    '15 matches in 8 files',
    {
      visible: [
        '15 matches in 8 files',
        'Button.tsx (1)',
        'Modal.tsx (1)',
        'Card.tsx (1)',
        '...+2 matches (ctrl+o to toggle)',
      ],
      hidden: ['App.tsx'],
    },
    ['multiple-files', 'match-lines', 'collapsed'],
    viewport
  ),
};

// With detailed match content
export const WithMatchContent = {
  render: () => (
    <Grep
      content={JSON.stringify({ pattern: 'TODO' })}
      status="success"
      isFinished={true}
      result={{
        status: 'success',
        output: {
          items: [
            {
              Json: {
                numMatches: 5,
                numFiles: 3,
                truncated: false,
                results: [
                  {
                    file: 'src/utils/helpers.ts',
                    count: 2,
                    matches: [
                      '45:// TODO: Add error handling',
                      '78:// TODO: Optimize this function',
                    ],
                  },
                  {
                    file: 'src/components/Form.tsx',
                    count: 2,
                    matches: [
                      '23:// TODO: Add validation',
                      '89:// TODO: Handle edge cases',
                    ],
                  },
                  {
                    file: 'src/api/client.ts',
                    count: 1,
                    matches: ['12:// TODO: Add retry logic'],
                  },
                ],
              },
            },
          ],
        },
      }}
    />
  ),
  parameters: certifyVisualStory(
    'TODO: Add retry logic',
    {
      visible: [
        '5 matches in 3 files',
        'helpers.ts (2)',
        'Form.tsx (2)',
        'client.ts (1)',
        '12:// TODO: Add retry logic',
      ],
      ordered: ['helpers.ts (2)', 'Form.tsx (2)', 'client.ts (1)'],
    },
    ['multiple-files', 'match-lines'],
    viewport
  ),
};

// Truncated results
export const Truncated = {
  args: {
    name: 'Grepped',
    content: JSON.stringify({ pattern: 'function' }),
    status: 'success',
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              numMatches: 250,
              numFiles: 45,
              truncated: true,
              results: [
                {
                  file: 'src/utils/math.ts',
                  count: 12,
                  matches: [
                    '5:function add(a, b) {',
                    '15:function subtract(a, b) {',
                  ],
                },
                {
                  file: 'src/utils/string.ts',
                  count: 8,
                  matches: ['3:function capitalize(str) {'],
                },
                {
                  file: 'src/utils/array.ts',
                  count: 6,
                  matches: ['7:function flatten(arr) {'],
                },
              ],
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
        '250 matches in 45 files (showing first results)',
        'math.ts (12)',
        'array.ts (6)',
        '(ctrl+o to toggle)',
      ],
    },
    ['multiple-files', 'match-lines', 'truncated'],
    viewport
  ),
};

// Error state
export const Error = {
  args: {
    name: 'Grepped',
    content: JSON.stringify({ pattern: '[invalid' }),
    status: 'error',
    isFinished: true,
    result: {
      status: 'error',
      error: "Invalid regex '[invalid': unclosed character class",
    },
  },
  parameters: certifyVisualStory(
    'Invalid regex',
    {
      visible: [
        'Grep "[invalid"',
        "Invalid regex '[invalid': unclosed character class",
      ],
    },
    ['error'],
    viewport
  ),
};

// Standalone without StatusBar wrapper
export const Standalone = {
  args: {
    name: 'Grepped',
    content: JSON.stringify({ pattern: 'test' }),
    noStatusBar: true,
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [
          {
            Json: {
              numMatches: 5,
              numFiles: 2,
              truncated: false,
              results: [
                { file: 'src/test/utils.test.ts', count: 3 },
                { file: 'src/test/api.test.ts', count: 2 },
              ],
            },
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'utils.test.ts',
    {
      visible: [
        'Grep "test"',
        '5 matches in 2 files',
        'utils.test.ts (3)',
        'api.test.ts (2)',
      ],
    },
    ['multiple-files', 'standalone'],
    viewport
  ),
};

export const KasTextResult = {
  args: {
    content: JSON.stringify({ query: 'TODO' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        message: [
          'You searched for TODO and received the following results:',
          'src/index.ts',
          '12:// TODO: wire command',
          'src/app.ts',
          '8:// TODO: add fallback',
          'src/config.ts',
          '3:// TODO: document option',
        ].join('\n'),
      },
    },
  },
  parameters: certifyVisualStory(
    'config.ts',
    {
      visible: [
        'Grep "TODO"',
        '3 matches in 3 files',
        'index.ts (1)',
        'app.ts (1)',
        'config.ts (1)',
      ],
      hidden: ['You searched for', 'query=TODO'],
    },
    ['multiple-files', 'match-lines', 'kas-text'],
    viewport
  ),
};

export const StaticSummary = {
  args: {
    content: JSON.stringify({ pattern: 'STATIC_MARKER' }),
    isFinished: true,
    isStatic: true,
    result: {
      status: 'success',
      output: {
        numMatches: 2,
        numFiles: 1,
        truncated: false,
        results: [
          {
            file: 'src/history.ts',
            count: 2,
            matches: ['1:STATIC_MARKER', '2:STATIC_MARKER'],
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    '2 matches in 1 file',
    {
      visible: ['Grep "STATIC_MARKER"', '2 matches in 1 file'],
      hidden: ['history.ts', '1:STATIC_MARKER', 'ctrl+o'],
    },
    ['single-file', 'static'],
    viewport
  ),
};

export const OutputTree = {
  args: {
    content: JSON.stringify({ pattern: 'PORT_MARKER' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        numMatches: 1,
        numFiles: 1,
        truncated: false,
        results: [
          {
            file: 'src/port.ts',
            count: 1,
            matches: ['7:const PORT_MARKER = true;'],
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'PORT_MARKER',
    {
      visible: [
        'Grep "PORT_MARKER"',
        'output:',
        '1 match in 1 file',
        'port.ts (1)',
        '7:const PORT_MARKER = true;',
      ],
      ordered: [
        'Grep "PORT_MARKER"',
        'output:',
        '1 match in 1 file',
        'port.ts (1)',
      ],
    },
    ['single-file', 'match-lines', 'output-tree'],
    viewport,
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};
