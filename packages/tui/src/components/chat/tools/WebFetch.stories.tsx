import { WebFetch } from './WebFetch.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 110, rows: 14 };

function textResult(text: string) {
  return {
    status: 'success' as const,
    output: { items: [{ Text: text }] },
  };
}

const meta = {
  component: WebFetch,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: { label: 'Web fetch in progress' },
      summary: { label: 'Fetched content-size summary' },
      'long-url': { label: 'Long URL is truncated for display' },
      options: { label: 'Fetch mode and search terms' },
      root: { label: 'Root URL omits a trailing slash' },
      'query-omitted': {
        label: 'Display target omits URL query and fragment',
      },
      'malformed-url': { label: 'Malformed URL error' },
      'http-error': { label: 'HTTP status error' },
      'summary-clipped': {
        label: 'Summary respects display character cap',
        gapType: 'integration-only',
        description: 'The display-policy provider owns summary clipping.',
      },
      'summary-filtered': {
        label: 'Summary hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
      'hyperlink-destination': {
        label: 'OSC-8 target opens the complete URL',
        gapType: 'integration-only',
        description:
          'Hyperlink destination and clickability require terminal integration.',
      },
    },
    storyOrder: [
      'Fetching',
      'FetchedWithSummary',
      'LongUrl',
      'WithOptions',
      'RootUrl',
      'QueryAndFragment',
      'MalformedUrl',
      'HttpError',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Fetching = {
  args: {
    content: JSON.stringify({ url: 'https://bun.sh/blog/bun-v1.2.20' }),
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'bun.sh/blog/bun-v1.2.20',
    {
      visible: ['WebFetch bun.sh/blog/bun-v1.2.20'],
      hidden: ['chars'],
    },
    ['loading'],
    viewport
  ),
};

export const FetchedWithSummary = {
  args: {
    content: JSON.stringify({ url: 'https://bun.sh/blog/bun-v1.2.20' }),
    isFinished: true,
    result: textResult('FETCH_BODY'),
  },
  parameters: certifyVisualStory(
    '10 chars',
    {
      visible: ['WebFetch bun.sh/blog/bun-v1.2.20', '10 chars'],
      ordered: ['WebFetch bun.sh/blog/bun-v1.2.20', '10 chars'],
    },
    ['summary'],
    viewport
  ),
};

export const LongUrl = {
  args: {
    content: JSON.stringify({
      url: 'https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Promise/allSettled',
    }),
    isFinished: true,
    result: textResult('LONG_BODY'),
  },
  parameters: certifyVisualStory(
    'developer.mozilla.org',
    {
      visible: [
        'WebFetch developer.mozilla.org/en-US/docs/Web/JavaScript/Reference...',
        '9 chars',
      ],
      hidden: ['Global_Objects'],
    },
    ['summary', 'long-url'],
    viewport
  ),
};

export const WithOptions = {
  args: {
    content: JSON.stringify({
      url: 'https://example.com/api/docs',
      mode: 'truncated',
      search_terms: 'auth token',
    }),
    isFinished: true,
    result: textResult('AUTH_BODY'),
  },
  parameters: certifyVisualStory(
    'auth token',
    {
      visible: [
        'WebFetch example.com/api/docs',
        'mode=truncated',
        'search_terms=auth token',
        '9 chars',
      ],
      ordered: [
        'WebFetch example.com/api/docs',
        'mode=truncated',
        'search_terms=auth token',
        '9 chars',
      ],
    },
    ['summary', 'options'],
    viewport
  ),
};

export const RootUrl = {
  args: {
    content: JSON.stringify({ url: 'https://example.com/' }),
    isFinished: true,
    result: textResult('ROOT'),
  },
  parameters: certifyVisualStory(
    'WebFetch example.com',
    {
      visible: ['WebFetch example.com', '4 chars'],
      hidden: ['example.com/'],
    },
    ['summary', 'root'],
    viewport
  ),
};

export const QueryAndFragment = {
  args: {
    content: JSON.stringify({
      url: 'https://example.com/api?token=secret#section',
    }),
    isFinished: true,
    result: textResult('QUERY_BODY'),
  },
  parameters: certifyVisualStory(
    'WebFetch example.com/api',
    {
      visible: ['WebFetch example.com/api', '10 chars'],
      hidden: ['token=secret', '#section'],
    },
    ['summary', 'query-omitted'],
    viewport
  ),
};

export const MalformedUrl = {
  args: {
    content: JSON.stringify({ url: 'not a url' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'Failed to fetch URL not a url',
    },
  },
  parameters: certifyVisualStory(
    'Failed to fetch',
    {
      visible: ['WebFetch not a url', 'Failed to fetch URL not a url'],
      ordered: ['WebFetch not a url', 'Failed to fetch URL not a url'],
    },
    ['malformed-url'],
    viewport
  ),
};

export const HttpError = {
  args: {
    content: JSON.stringify({ url: 'https://example.com/missing' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'HTTP error 404: https://example.com/missing',
    },
  },
  parameters: certifyVisualStory(
    'HTTP error 404',
    {
      visible: [
        'WebFetch example.com/missing',
        'HTTP error 404: https://example.com/missing',
      ],
      ordered: [
        'WebFetch example.com/missing',
        'HTTP error 404: https://example.com/missing',
      ],
    },
    ['http-error'],
    viewport
  ),
};
