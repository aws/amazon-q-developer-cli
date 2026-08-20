import { WebSearch } from './WebSearch.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 110, rows: 12 };

const meta = {
  component: WebSearch,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: {
        label: 'Web search in-progress indicator',
        gapType: 'visual-baseline-required',
        description:
          'Loading differs only through animated status styling, which text assertions cannot prove.',
      },
      completed: {
        label: 'Web search completed indicator',
        gapType: 'visual-baseline-required',
        description:
          'Completion differs only through status styling, which text assertions cannot prove.',
      },
      'text-summary': { label: 'Text-envelope result summary' },
      'json-summary': { label: 'JSON-envelope result summary' },
      'large-summary': { label: 'Thousand-character summary formatting' },
      'empty-query': { label: 'Empty query uses the web fallback target' },
      error: { label: 'Web search error' },
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
    },
    storyOrder: [
      'Searching',
      'Searched',
      'TextSummary',
      'JsonSummary',
      'LargeSummary',
      'EmptyQuery',
      'Error',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Searching = {
  args: {
    content: JSON.stringify({ query: 'bun release notes' }),
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'bun release notes',
    {
      visible: ['WebSearch "bun release notes"'],
      hidden: ['chars'],
    },
    [],
    viewport
  ),
};

export const Searched = {
  args: {
    content: JSON.stringify({ query: 'react terminal ui' }),
    isFinished: true,
  },
  parameters: certifyVisualStory(
    'react terminal ui',
    {
      visible: ['WebSearch "react terminal ui"'],
      hidden: ['chars'],
    },
    [],
    viewport
  ),
};

export const TextSummary = {
  args: {
    content: JSON.stringify({ query: 'typescript features' }),
    isFinished: true,
    result: {
      status: 'success',
      output: { items: [{ Text: '1234567890' }] },
    },
  },
  parameters: certifyVisualStory(
    '10 chars',
    {
      visible: ['WebSearch "typescript features"', '10 chars'],
      ordered: ['WebSearch "typescript features"', '10 chars'],
    },
    ['text-summary'],
    viewport
  ),
};

export const JsonSummary = {
  args: {
    content: JSON.stringify({ query: 'rust async runtime' }),
    isFinished: true,
    result: {
      status: 'success',
      output: {
        items: [{ Json: { text: 'JSON_RESULT_BODY' } }],
      },
    },
  },
  parameters: certifyVisualStory(
    '16 chars',
    {
      visible: ['WebSearch "rust async runtime"', '16 chars'],
      hidden: ['JSON_RESULT_BODY', '[object Object]'],
    },
    ['json-summary'],
    viewport
  ),
};

export const LargeSummary = {
  args: {
    content: JSON.stringify({ query: 'large result boundary' }),
    isFinished: true,
    result: {
      status: 'success',
      output: { items: [{ Text: 'x'.repeat(1000) }] },
    },
  },
  parameters: certifyVisualStory(
    '1.0k chars',
    {
      visible: ['WebSearch "large result boundary"', '1.0k chars'],
      hidden: ['1000 chars'],
    },
    ['text-summary', 'large-summary'],
    viewport
  ),
};

export const EmptyQuery = {
  args: {
    content: JSON.stringify({ query: '' }),
    isFinished: true,
  },
  parameters: certifyVisualStory(
    'the web',
    {
      visible: ['WebSearch the web'],
      hidden: ['WebSearch ""', 'chars'],
    },
    ['empty-query'],
    viewport
  ),
};

export const Error = {
  args: {
    content: JSON.stringify({ query: 'some failing search' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'Web search failed: rate limit exceeded',
    },
  },
  parameters: certifyVisualStory(
    'rate limit exceeded',
    {
      visible: [
        'WebSearch "some failing search"',
        'Web search failed: rate limit exceeded',
      ],
      ordered: [
        'WebSearch "some failing search"',
        'Web search failed: rate limit exceeded',
      ],
    },
    ['error'],
    viewport
  ),
};
