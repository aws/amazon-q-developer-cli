import { Code } from './Code.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 110, rows: 16 };

const meta = {
  component: Code,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: { label: 'Code operation in progress' },
      completed: { label: 'Code operation completed' },
      'symbol-target': { label: 'Symbol target' },
      'pattern-target': { label: 'Pattern target' },
      'file-target': { label: 'File target' },
      arguments: { label: 'Additional operation arguments' },
      symbols: { label: 'Symbol result text envelope' },
      references: { label: 'Reference result text envelope' },
      'text-summary': { label: 'Text result summary' },
      'no-result': { label: 'Successful operation with no matching result' },
      'verbose-suppressed': {
        label: 'Verbose overview suppresses duplicate summary output',
      },
      error: {
        label: 'Code operation error',
        gapType: 'integration-only',
        description:
          'The shared tool-result transport owns invocation failures; operation-level misses return successful text.',
      },
      static: { label: 'Past code operation suppresses transient summary' },
      rollout: {
        label: 'Rollout summary is not legacy-capped',
        gapType: 'integration-only',
        description:
          'The display-policy provider line cap takes precedence over the rollout fallback.',
      },
      'summary-truncation-signal': {
        label: 'Legacy capped summary reports hidden rows',
        gapType: 'product-limitation',
        description:
          'The default display policy caps summaries without an overflow hint.',
      },
      'windows-file-target': {
        label: 'Windows file target displays only its basename',
        gapType: 'product-limitation',
        description: 'The target formatter splits only POSIX path separators.',
      },
      'output-filtered': {
        label: 'Summary hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
      'output-capped': {
        label: 'Default display policy caps the summary at five lines',
      },
    },
    storyOrder: [
      'SearchingSymbols',
      'SearchedSymbols',
      'FoundReferences',
      'PatternSearch',
      'FileOperation',
      'TextSummary',
      'VerboseOverview',
      'MissingDefinition',
      'StaticHistory',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const SearchingSymbols = {
  args: {
    noStatusBar: true,
    isFinished: false,
    content: JSON.stringify({
      operation: 'search_symbols',
      symbol_name: 'WorkflowController',
    }),
  },
  parameters: certifyVisualStory(
    'Searching symbols',
    {
      visible: ['Code WorkflowController', 'Searching symbols'],
      hidden: ['Searched symbols'],
      ordered: ['Code WorkflowController', 'Searching symbols'],
    },
    ['loading', 'symbol-target'],
    viewport
  ),
};

export const SearchedSymbols = {
  args: {
    noStatusBar: true,
    isFinished: true,
    content: JSON.stringify({
      operation: 'search_symbols',
      symbol_name: 'Workflow',
    }),
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: '[Class WorkflowController @ src/workflow/controller.ts:12-80 | WorkflowController, Function WorkflowStore @ src/stores/workflow-store.ts:20-95 | WorkflowStore, Function WorkflowMonitor @ src/components/WorkflowMonitor.tsx:15-70 | WorkflowMonitor, Type WorkflowNode @ src/types/workflow.ts:8-18 | WorkflowNode]',
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'WorkflowMonitor',
    {
      visible: [
        'Code Workflow',
        'Searched symbols',
        'WorkflowController',
        'WorkflowStore',
        'WorkflowMonitor',
      ],
      hidden: ['Searching symbols'],
      ordered: [
        'Searched symbols',
        'WorkflowController',
        'WorkflowStore',
        'WorkflowMonitor',
      ],
    },
    ['completed', 'symbol-target', 'symbols'],
    viewport
  ),
};

export const FoundReferences = {
  args: {
    noStatusBar: true,
    isFinished: true,
    content: JSON.stringify({
      operation: 'find_references',
      file_path: 'src/stores/app-store.ts',
      row: 42,
      column: 5,
    }),
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: '[src/index.ts:10:4, src/app.ts:25:8, src/store.ts:40:2, src/test.ts:12:6]',
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'src/test.ts:12:6',
    {
      visible: [
        'Code app-store.ts',
        'Found references',
        'src/index.ts:10:4',
        'src/test.ts:12:6',
      ],
    },
    ['completed', 'file-target', 'references'],
    viewport
  ),
};

export const PatternSearch = {
  args: {
    noStatusBar: true,
    isFinished: true,
    content: JSON.stringify({
      operation: 'pattern_search',
      pattern: 'useState($$$ARGS)',
      language: 'tsx',
    }),
    result: {
      status: 'success',
      output: { items: [{ Text: 'No pattern matches found' }] },
    },
  },
  parameters: certifyVisualStory(
    'No pattern matches found',
    {
      visible: [
        'Code "useState($$$ARGS)"',
        'Pattern searched',
        'language=tsx',
        'No pattern matches found',
      ],
      ordered: [
        'Code "useState($$$ARGS)"',
        'Pattern searched',
        'language=tsx',
        'No pattern matches found',
      ],
    },
    ['completed', 'pattern-target', 'arguments', 'text-summary', 'no-result'],
    viewport
  ),
};

export const FileOperation = {
  args: {
    noStatusBar: true,
    isFinished: true,
    content: JSON.stringify({
      operation: 'get_document_symbols',
      file_path: 'src/components/App.tsx',
    }),
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: '[Function App @ src/components/App.tsx:1-20 | App, Function AppContainer @ src/components/App.tsx:22-60 | AppContainer]',
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'AppContainer',
    {
      visible: [
        'Code App.tsx',
        'Got symbols',
        'src/components/App.tsx',
        'App',
        'AppContainer',
      ],
    },
    ['completed', 'file-target', 'symbols', 'text-summary'],
    viewport
  ),
};

const textOutput = [
  'Scoped to: src/renderer.ts',
  '[Function renderFrame @ src/renderer.ts:18-22 | export function renderFrame() {',
  'export function renderFrame() {',
  '  const renderer = createRenderer();',
  '  const output = renderer.render();',
  '  return output;',
  '}]',
];

export const TextSummary = {
  args: {
    noStatusBar: true,
    isFinished: true,
    content: JSON.stringify({
      operation: 'lookup_symbols',
      symbols: ['renderFrame'],
      include_source: true,
      file_path: 'src/renderer.ts',
    }),
    result: {
      status: 'success',
      output: { items: [{ Text: textOutput.join('\n') }] },
    },
  },
  parameters: certifyVisualStory(
    'export function renderFrame() {',
    {
      visible: [
        'Looked up symbols',
        'Scoped to: src/renderer.ts',
        'export function renderFrame() {',
        'const renderer = createRenderer();',
        'const output = renderer.render();',
      ],
      hidden: ['return output;', '}]'],
      ordered: [
        'Looked up symbols',
        'Scoped to: src/renderer.ts',
        'export function renderFrame() {',
        'const renderer = createRenderer();',
        'const output = renderer.render();',
      ],
    },
    ['completed', 'file-target', 'text-summary', 'output-capped'],
    viewport
  ),
};

const overviewOutput = JSON.stringify({
  workspace_path: '/workspace/kiro-cli',
  size_category: 'L',
  summary: {
    total_files: 320,
    prioritized_files: 120,
    f: 840,
    c: 190,
    i: 0,
    m: 0,
    k: 0,
    loc: 68000,
  },
  symbol_key: {
    f: 'Functions',
    c: 'Classes/Structs/Enums',
    i: 'Interfaces/Traits',
    m: 'Modules',
    k: 'Constants/Statics',
    loc: 'Lines of Code',
  },
  truncated: false,
  packages: {
    src: {
      'main.rs': {
        f: ['main'],
        c: [],
        loc: 42,
        score: '88.0',
      },
    },
  },
});

export const VerboseOverview = {
  args: {
    noStatusBar: true,
    isFinished: true,
    content: JSON.stringify({ operation: 'generate_codebase_overview' }),
    result: {
      status: 'success',
      output: { items: [{ Text: overviewOutput }] },
    },
  },
  parameters: certifyVisualStory(
    'Generated overview',
    {
      visible: ['Code generate_codebase_overview', 'Generated overview'],
      hidden: ['"workspace_path":"/workspace/kiro-cli"'],
    },
    ['completed', 'verbose-suppressed'],
    viewport
  ),
};

export const MissingDefinition = {
  args: {
    noStatusBar: true,
    isFinished: true,
    content: JSON.stringify({
      operation: 'goto_definition',
      file_path: 'src/components/Missing.tsx',
      row: 1,
      column: 1,
    }),
    result: {
      status: 'success',
      output: { items: [{ Text: 'No definition found' }] },
    },
  },
  parameters: certifyVisualStory(
    'No definition found',
    {
      visible: [
        'Code Missing.tsx',
        'Went to definition',
        'No definition found',
      ],
    },
    ['completed', 'file-target', 'text-summary', 'no-result'],
    viewport
  ),
};

export const StaticHistory = {
  args: {
    noStatusBar: true,
    isFinished: true,
    isStatic: true,
    content: JSON.stringify({
      operation: 'get_hover',
      file_path: 'src/renderer.ts',
      row: 18,
      column: 7,
    }),
    result: {
      status: 'success',
      output: {
        items: [
          {
            Text: 'HoverInfo { file_path: "src/renderer.ts", row: 18, column: 7, content: Some("Renderer entry point") }',
          },
        ],
      },
    },
  },
  parameters: certifyVisualStory(
    'Got hover info',
    {
      visible: ['Code renderer.ts', 'Got hover info'],
      hidden: ['HoverInfo {'],
    },
    ['completed', 'file-target', 'static'],
    viewport
  ),
};
