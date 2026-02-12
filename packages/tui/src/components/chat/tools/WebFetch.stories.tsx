import { WebFetch } from './WebFetch.js';

const meta = {
  component: WebFetch,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Fetching',
      'Fetched',
      'FetchedWithSummary',
      'LongUrl',
      'WithMode',
      'NoUrl',
      'Error',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

// Fetching state (in progress)
export const Fetching = {
  args: {
    content: JSON.stringify({ url: 'https://bun.sh/blog/bun-v1.2.20' }),
    isFinished: false,
  },
};

// Fetched state (finished, no result yet)
export const Fetched = {
  args: {
    content: JSON.stringify({ url: 'https://docs.github.com/en/rest' }),
    isFinished: true,
  },
};

// Fetched with result summary showing content size
export const FetchedWithSummary = {
  args: {
    content: JSON.stringify({ url: 'https://bun.sh/blog/bun-v1.2.20' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{
          Text: 'Bun v1.2.20 release notes. This release fixes 141 issues and includes many reliability improvements throughout the runtime, the bundler, and the dev server. Reduced idle CPU usage, automatic yarn.lock migration, 40x faster AbortSignal.timeout...',
        }],
      },
    },
  },
};

// Long URL gets truncated
export const LongUrl = {
  args: {
    content: JSON.stringify({
      url: 'https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled',
    }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{
          Text: 'The Promise.allSettled() static method takes an iterable of promises as input and returns a single Promise.',
        }],
      },
    },
  },
};

// With explicit mode
export const WithMode = {
  args: {
    content: JSON.stringify({
      url: 'https://example.com/api/docs',
      mode: 'truncated',
    }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{
          Text: 'API documentation for the example service. Authentication: Bearer token required for all endpoints.',
        }],
      },
    },
  },
};

// No URL provided
export const NoUrl = {
  args: {
    content: JSON.stringify({}),
    isFinished: true,
  },
};

// Error state
export const Error = {
  args: {
    content: JSON.stringify({ url: 'https://nonexistent.example.com/page' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'HTTP error 404: https://nonexistent.example.com/page',
    },
  },
};
