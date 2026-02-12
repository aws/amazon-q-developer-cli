import { WebSearch } from './WebSearch.js';

const meta = {
  component: WebSearch,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Searching',
      'Searched',
      'SearchedWithSummary',
      'NoQuery',
      'Error',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// Searching state (in progress)
export const Searching = {
  args: {
    content: JSON.stringify({ query: 'bun v1.2.20 release notes' }),
    isFinished: false,
  },
};

// Searched state (finished, no result yet)
export const Searched = {
  args: {
    content: JSON.stringify({ query: 'react ink terminal ui framework' }),
    isFinished: true,
  },
};

// Searched with result summary
export const SearchedWithSummary = {
  args: {
    content: JSON.stringify({ query: 'typescript 5.9 new features' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{
          Text: 'TypeScript 5.9 introduces several new features including...\nImproved type inference for generic functions\nNew satisfies operator enhancements\nBetter error messages for template literal types',
        }],
      },
    },
  },
};

// No query provided
export const NoQuery = {
  args: {
    content: JSON.stringify({}),
    isFinished: true,
  },
};

// Error state
export const Error = {
  args: {
    content: JSON.stringify({ query: 'some failing search' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'Web search failed: rate limit exceeded',
    },
  },
};
