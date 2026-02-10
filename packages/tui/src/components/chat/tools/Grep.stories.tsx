import { Grep } from './Grep.js';

const meta = {
  component: Grep,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Grepping',
      'NoMatches',
      'SingleFileMatch',
      'MultipleFileMatches',
      'WithMatchContent',
      'Truncated',
      'Error',
      'Standalone',
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
        items: [{
          Json: {
            numMatches: 0,
            numFiles: 0,
            truncated: false,
            message: 'No matches found for pattern: nonexistentPattern',
          },
        }],
      },
    },
  },
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
        items: [{
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
        }],
      },
    },
  },
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
        items: [{
          Json: {
            numMatches: 15,
            numFiles: 8,
            truncated: false,
            results: [
              { file: 'src/components/Button.tsx', count: 1, matches: ["1:import React from 'react';"] },
              { file: 'src/components/Modal.tsx', count: 1, matches: ["1:import React, { useState } from 'react';"] },
              { file: 'src/components/Card.tsx', count: 1, matches: ["1:import React from 'react';"] },
              { file: 'src/hooks/useTheme.ts', count: 1, matches: ["1:import { useContext } from 'react';"] },
              { file: 'src/App.tsx', count: 1, matches: ["1:import React from 'react';"] },
            ],
          },
        }],
      },
    },
  },
};

// With detailed match content
export const WithMatchContent = {
  render: () => (
    <Grep
      name="Grepped"
      content={JSON.stringify({ pattern: 'TODO' })}
      status="success"
      isFinished={true}
      result={{
        status: 'success',
        output: {
          items: [{
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
                  matches: [
                    '12:// TODO: Add retry logic',
                  ],
                },
              ],
            },
          }],
        },
      }}
    />
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
        items: [{
          Json: {
            numMatches: 250,
            numFiles: 45,
            truncated: true,
            results: [
              { file: 'src/utils/math.ts', count: 12, matches: ['5:function add(a, b) {', '15:function subtract(a, b) {'] },
              { file: 'src/utils/string.ts', count: 8, matches: ['3:function capitalize(str) {'] },
              { file: 'src/utils/array.ts', count: 6, matches: ['7:function flatten(arr) {'] },
            ],
          },
        }],
      },
    },
  },
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
        items: [{
          Json: {
            numMatches: 5,
            numFiles: 2,
            truncated: false,
            results: [
              { file: 'src/test/utils.test.ts', count: 3 },
              { file: 'src/test/api.test.ts', count: 2 },
            ],
          },
        }],
      },
    },
  },
};
