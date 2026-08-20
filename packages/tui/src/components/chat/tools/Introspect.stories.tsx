import { Introspect } from './Introspect.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 110, rows: 18 };

const documentationResult = {
  status: 'success' as const,
  output: {
    items: [
      {
        Json: {
          documentation: [
            'DOC_LINE_1 overview',
            'DOC_LINE_2 syntax',
            'DOC_LINE_3 parameters',
            'DOC_LINE_4 examples',
            'DOC_LINE_5 constraints',
            'DOC_LINE_6 caveats',
            'DOC_LINE_7 references',
          ].join('\n'),
          query_context: 'tool docs',
        },
      },
    ],
  },
};

const meta = {
  component: Introspect,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: {
        label: 'Documentation lookup in-progress indicator',
        gapType: 'visual-baseline-required',
        description:
          'Loading differs only through animated status styling, which text assertions cannot prove.',
      },
      completed: {
        label: 'Documentation lookup completed indicator',
        gapType: 'visual-baseline-required',
        description:
          'Completion differs only through status styling, which text assertions cannot prove.',
      },
      query: { label: 'Lookup by natural-language query' },
      'doc-path': { label: 'Lookup by documentation path' },
      'no-target': { label: 'Lookup without a target' },
      error: { label: 'Missing documentation path' },
      'header-only': { label: 'Default cohort suppresses documentation body' },
      'output-tree': { label: 'Rollout documentation output preview' },
      expanded: {
        label: 'Expanded documentation output',
        gapType: 'integration-only',
        description:
          'Expansion is owned by the global tool-output controller, not Introspect.',
      },
      'output-filtered': {
        label: 'Documentation hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
      'static-persistence': {
        label: 'Past documentation follows persistence settings',
        gapType: 'integration-only',
        description: 'The display-policy provider owns static persistence.',
      },
      'argument-precedence': {
        label: 'UI target follows backend doc_path precedence',
        gapType: 'product-limitation',
        description:
          'The backend prioritizes doc_path while the UI currently displays query.',
      },
    },
    storyOrder: [
      'Introspecting',
      'IntrospectedQuery',
      'IntrospectedDocPath',
      'NoQuery',
      'MissingDocument',
      'DocumentationOffCohort',
      'DocumentationOutputTree',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Introspecting = {
  args: {
    content: JSON.stringify({ query: 'MCP governance' }),
    isFinished: false,
  },
  parameters: certifyVisualStory(
    'MCP governance',
    {
      visible: ['Introspect MCP governance'],
      hidden: ['output:', 'DOC_LINE_1'],
    },
    ['query'],
    viewport
  ),
};

export const IntrospectedQuery = {
  args: {
    content: JSON.stringify({ query: 'how to use slash commands' }),
    isFinished: true,
  },
  parameters: certifyVisualStory(
    'slash commands',
    {
      visible: ['Introspect how to use slash commands'],
      hidden: ['output:', 'DOC_LINE_1'],
    },
    ['query'],
    viewport
  ),
};

export const IntrospectedDocPath = {
  args: {
    content: JSON.stringify({ doc_path: 'features/tangent-mode.md' }),
    isFinished: true,
  },
  parameters: certifyVisualStory(
    'tangent-mode.md',
    {
      visible: ['Introspect features/tangent-mode.md'],
      hidden: ['output:', 'DOC_LINE_1'],
    },
    ['doc-path'],
    viewport
  ),
};

export const NoQuery = {
  args: {
    content: JSON.stringify({}),
    isFinished: true,
  },
  parameters: certifyVisualStory(
    'Introspect',
    {
      visible: ['Introspect'],
      hidden: ['output:', 'DOC_LINE_1'],
    },
    ['no-target'],
    viewport
  ),
};

export const MissingDocument = {
  args: {
    content: JSON.stringify({ doc_path: 'missing.md' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'Document not found: missing.md',
    },
  },
  parameters: certifyVisualStory(
    'Document not found',
    {
      visible: ['Introspect missing.md', 'Document not found: missing.md'],
      ordered: ['Introspect missing.md', 'Document not found: missing.md'],
    },
    ['doc-path', 'error'],
    viewport
  ),
};

export const DocumentationOffCohort = {
  args: {
    content: JSON.stringify({ query: 'tool docs' }),
    isFinished: true,
    result: documentationResult,
  },
  parameters: certifyVisualStory(
    'tool docs',
    {
      visible: ['Introspect tool docs'],
      hidden: ['output:', 'DOC_LINE_1'],
    },
    ['query', 'header-only'],
    viewport
  ),
};

export const DocumentationOutputTree = {
  args: {
    content: JSON.stringify({ query: 'tool docs' }),
    isFinished: true,
    result: documentationResult,
  },
  parameters: certifyVisualStory(
    'DOC_LINE_5',
    {
      visible: [
        'Introspect tool docs',
        'output:',
        'DOC_LINE_1 overview',
        'DOC_LINE_5 constraints',
        '...+2 lines (ctrl+o to toggle)',
      ],
      hidden: ['DOC_LINE_6 caveats', 'DOC_LINE_7 references'],
      ordered: [
        'Introspect tool docs',
        'output:',
        'DOC_LINE_1 overview',
        'DOC_LINE_5 constraints',
      ],
    },
    ['query', 'output-tree'],
    viewport,
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};
