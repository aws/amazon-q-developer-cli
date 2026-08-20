import { Tool } from './Tool.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 110, rows: 18 };
const longOutput = [
  'Match 1: src/utils.ts:10 - export function helper()',
  'Match 2: src/utils.ts:25 - export function format()',
  'Match 3: src/utils.ts:40 - export function parse()',
  'Match 4: src/utils.ts:55 - export function validate()',
  'Match 5: src/utils.ts:70 - export function transform()',
  'Match 6: src/utils.ts:85 - export function serialize()',
  'Match 7: src/utils.ts:100 - export function deserialize()',
].join('\n');

const meta = {
  component: Tool,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: {
        label: 'Generic tool in-progress indicator',
        gapType: 'visual-baseline-required',
        description:
          'Loading differs only through animated status styling, which text assertions cannot prove.',
      },
      completed: {
        label: 'Generic tool completed indicator',
        gapType: 'visual-baseline-required',
        description:
          'Completion differs only through status styling, which text assertions cannot prove.',
      },
      location: { label: 'Single source location' },
      'multiple-locations': { label: 'Multiple source locations' },
      arguments: { label: 'Formatted tool arguments' },
      'short-output': { label: 'Short text result' },
      'collapsed-output': { label: 'Collapsed long text result' },
      'structured-json': { label: 'Structured JSON result' },
      'output-tree': { label: 'Rollout output-tree presentation' },
      static: { label: 'Past generic tool suppresses transient output' },
      error: {
        label: 'Generic tool error',
        gapType: 'integration-only',
        description:
          'ToolUseMessage routes generic failures through FallbackError before Tool.',
      },
      expanded: {
        label: 'Expanded generic tool output',
        gapType: 'integration-only',
        description:
          'Expansion is owned by the global tool-output controller, not Tool.',
      },
      'output-filtered': {
        label: 'Output hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
      'static-persistence': {
        label: 'Past output retained by persistence settings',
        gapType: 'integration-only',
        description: 'The display-policy provider owns static persistence.',
      },
      'windows-location': {
        label: 'Windows path displays only its basename',
        gapType: 'product-limitation',
        description:
          'The location formatter currently splits only POSIX path separators.',
      },
    },
    storyOrder: [
      'Using',
      'Completed',
      'SingleLocation',
      'MultipleLocations',
      'WithArguments',
      'ShortOutput',
      'LongOutput',
      'StructuredJson',
      'RolloutOutputTree',
      'StaticHistory',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Using = {
  args: {
    name: 'custom_lookup',
    noStatusBar: true,
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'custom_lookup',
    {
      visible: ['custom_lookup'],
      hidden: ['output:', 'ctrl+o'],
    },
    [],
    viewport
  ),
};

export const Completed = {
  args: {
    name: 'custom_lookup',
    noStatusBar: true,
    isFinished: true,
  },
  parameters: certifyVisualStory(
    'custom_lookup',
    {
      visible: ['custom_lookup'],
      hidden: ['output:', 'ctrl+o'],
    },
    [],
    viewport
  ),
};

export const SingleLocation = {
  args: {
    name: 'grep_search',
    noStatusBar: true,
    isFinished: true,
    locations: [{ path: 'src/components/Button.tsx', line: 42 }],
  },
  parameters: certifyVisualStory(
    'Button.tsx:42',
    {
      visible: ['grep_search', 'Button.tsx:42'],
      hidden: ['src/components/Button.tsx'],
      ordered: ['grep_search', 'Button.tsx:42'],
    },
    ['location'],
    viewport
  ),
};

export const MultipleLocations = {
  args: {
    name: 'find_references',
    noStatusBar: true,
    isFinished: true,
    locations: [
      { path: 'src/index.ts', line: 10 },
      { path: 'src/utils/helpers.ts', line: 25 },
      { path: 'src/components/App.tsx', line: 5 },
      { path: 'src/types/index.ts', line: 1 },
      { path: 'src/hooks/useAuth.ts', line: 15 },
    ],
  },
  parameters: certifyVisualStory(
    'useAuth.ts:15',
    {
      visible: [
        'find_references',
        'index.ts:10',
        'helpers.ts:25',
        'App.tsx:5',
        'index.ts:1',
        'useAuth.ts:15',
      ],
      ordered: [
        'index.ts:10',
        'helpers.ts:25',
        'App.tsx:5',
        'index.ts:1',
        'useAuth.ts:15',
      ],
      occurrences: { 'index.ts': 2 },
    },
    ['multiple-locations'],
    viewport
  ),
};

export const WithArguments = {
  args: {
    name: 'custom_lookup',
    noStatusBar: true,
    isFinished: true,
    content: JSON.stringify({
      query: 'release workflow',
      limit: 10,
      includeArchived: false,
    }),
  },
  parameters: certifyVisualStory(
    'includeArchived=false',
    {
      visible: [
        'custom_lookup',
        'query=release workflow',
        'limit=10',
        'includeArchived=false',
      ],
      ordered: [
        'custom_lookup',
        'query=release workflow',
        'limit=10',
        'includeArchived=false',
      ],
    },
    ['arguments'],
    viewport
  ),
};

export const ShortOutput = {
  args: {
    name: 'web_search',
    noStatusBar: true,
    isFinished: true,
    result: {
      status: 'success',
      output: {
        text: [
          'Found 2 results:',
          '1. React documentation',
          '2. TypeScript handbook',
        ].join('\n'),
      },
    },
  },
  parameters: certifyVisualStory(
    'TypeScript handbook',
    {
      visible: [
        'web_search',
        'Found 2 results:',
        '1. React documentation',
        '2. TypeScript handbook',
      ],
      ordered: [
        'web_search',
        'Found 2 results:',
        '1. React documentation',
        '2. TypeScript handbook',
      ],
      hidden: ['ctrl+o'],
    },
    ['short-output'],
    viewport
  ),
};

export const LongOutput = {
  args: {
    name: 'custom_search',
    noStatusBar: true,
    isFinished: true,
    result: { status: 'success', output: { text: longOutput } },
  },
  parameters: certifyVisualStory(
    'Match 3',
    {
      visible: ['Match 1:', 'Match 2:', 'Match 3:', '...+4 lines'],
      hidden: ['Match 4:', 'Match 7:'],
      ordered: ['Match 1:', 'Match 2:', 'Match 3:', '...+4 lines'],
    },
    ['collapsed-output'],
    viewport
  ),
};

export const StructuredJson = {
  args: {
    name: 'custom_json',
    noStatusBar: true,
    isFinished: true,
    result: {
      status: 'success',
      output: { status: 'ok', count: 2 },
    },
  },
  parameters: certifyVisualStory(
    '"count": 2',
    {
      visible: [
        'custom_json',
        '{',
        '"status": "ok"',
        '"count": 2',
        '...+1 lines (ctrl+o to toggle)',
      ],
      hidden: ['[object Object]'],
      ordered: ['custom_json', '{', '"status": "ok"', '"count": 2'],
    },
    ['structured-json', 'collapsed-output'],
    viewport
  ),
};

export const RolloutOutputTree = {
  args: {
    name: 'custom_search',
    noStatusBar: true,
    isFinished: true,
    result: { status: 'success', output: { text: longOutput } },
  },
  parameters: certifyVisualStory(
    'Match 5',
    {
      visible: [
        'custom_search',
        'output:',
        'Match 1:',
        'Match 5:',
        '...+2 lines (ctrl+o to toggle)',
      ],
      hidden: ['Match 6:', 'Match 7:'],
      ordered: ['custom_search', 'output:', 'Match 1:', 'Match 5:'],
    },
    ['collapsed-output', 'output-tree'],
    viewport,
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};

export const StaticHistory = {
  args: {
    name: 'custom_search',
    noStatusBar: true,
    isFinished: true,
    isStatic: true,
    result: { status: 'success', output: { text: longOutput } },
  },
  parameters: certifyVisualStory(
    'custom_search',
    {
      visible: ['custom_search'],
      hidden: ['Match 1:', 'output:', 'ctrl+o'],
    },
    ['static'],
    viewport
  ),
};
