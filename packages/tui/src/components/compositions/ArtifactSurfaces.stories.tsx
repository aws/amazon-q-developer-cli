import React, { useState } from 'react';
import { Box } from '../../renderer.js';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
  type ArtifactGenerationEntry,
  type OpenArtifactView,
} from '../../stores/app-store.js';
import type {
  StorybookAssertions,
  StorybookParameters,
} from '../../storybook/contracts.js';
import type { ReviewAction } from '../../utils/spec-review/review-actions.js';
import type { ArtifactSummary } from '../../utils/spec-artifact-parser/index.js';
import { SpecReviewScreen } from '../layout/SpecReviewScreen.js';
import { ArtifactGenerationCard } from '../ui/ArtifactView/ArtifactGenerationCard.js';
import { ArtifactView } from '../ui/ArtifactView/index.js';
import { SpecCheckpointChip } from '../ui/SpecCheckpointChip.js';
import { SpecDescriptionIntro } from '../ui/SpecDescriptionIntro.js';

const viewport = { columns: 112, rows: 34 };
const featureName = 'release-certification';

const requirementsSummary: ArtifactSummary = {
  kind: 'requirements',
  items: [
    {
      number: 1,
      title: 'Fast certification feedback',
      userStory:
        '**User Story:** As a contributor, I want failed lanes identified quickly.',
      detailBody: [
        '### Requirement 1: Fast certification feedback',
        '',
        '**User Story:** As a contributor, I want failed lanes identified quickly.',
        '',
        '1. WHEN a lane fails THEN the report SHALL name the failing lane.',
      ].join('\n'),
    },
    {
      number: 2,
      title: 'Actionable evidence',
      userStory: null,
      detailBody: [
        '### Requirement 2: Actionable evidence',
        '',
        '1. WHEN certification completes THEN the report SHALL link evidence.',
      ].join('\n'),
    },
  ],
};

const designSummary: ArtifactSummary = {
  kind: 'design',
  overview:
    'Build once per platform, then fan out independent verification lanes.',
  overviewTruncated: false,
  sections: [
    {
      title: 'Architecture',
      detailBody: '## Architecture\n\nBuild and verification are separate.',
    },
    {
      title: 'Artifact contract',
      detailBody:
        '## Artifact contract\n\nEvery lane receives the same binary.',
    },
    {
      title: 'Failure isolation',
      detailBody: '## Failure isolation\n\nEach lane reports its own outcome.',
    },
  ],
};

const tasksSummary: ArtifactSummary = {
  kind: 'tasks',
  items: [
    {
      number: '1',
      title: 'Build platform assets',
      checked: true,
      subTasks: [
        { title: 'Build Linux binary', checked: true, depth: 2 },
        { title: 'Build Windows binary', checked: true, depth: 2 },
      ],
      detailBody: '- [x] 1. Build platform assets',
    },
    {
      number: '2',
      title: 'Run verification lanes',
      checked: false,
      subTasks: [
        { title: 'Run deterministic smoke tests', checked: true, depth: 2 },
        { title: 'Publish visual evidence', checked: false, depth: 2 },
      ],
      detailBody: '- [ ] 2. Run verification lanes',
    },
  ],
};

const bugfixSummary: ArtifactSummary = {
  kind: 'bugfix',
  overview:
    'A failed child lane can leave the aggregate certification status pending.',
  sections: [
    {
      title: 'Current Behavior (Defect)',
      clauses: [
        {
          number: '1.1',
          text: 'WHEN a child lane fails THEN the aggregate remains pending.',
        },
      ],
      detailBody: '### Current Behavior (Defect)',
    },
    {
      title: 'Expected Behavior',
      clauses: [
        {
          number: '2.1',
          text: 'WHEN a child lane fails THEN the aggregate reports that failure.',
        },
        {
          number: '2.2',
          text: 'WHEN the lane is retried successfully THEN the aggregate reports the recovered result.',
        },
      ],
      detailBody: '### Expected Behavior',
    },
  ],
};

const emptyRequirements: ArtifactSummary = {
  kind: 'requirements',
  items: [],
};

const stagedRequirementComment: ReviewAction = {
  kind: 'comment',
  id: 'visual-review-requirement',
  anchor: {
    range: { start: 4, end: 4 },
    heading: 'Requirement 1: Fast certification feedback',
    snippet:
      '1. WHEN a lane fails THEN the report SHALL name the failing lane.',
  },
  body: 'Name the failed platform as well as the lane.',
};

const stagedDesignComment: ReviewAction = {
  kind: 'comment',
  id: 'visual-review-design',
  anchor: {
    range: { start: 2, end: 2 },
    heading: 'Architecture',
    snippet: 'Build and verification are separate.',
  },
  body: 'Explain how the binary identity is verified.',
};

function createStoryStore(
  configure?: (store: AppStoreApi) => void
): AppStoreApi {
  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'v2',
    uiMode: 'tui',
  });
  store.setState({
    isInitialized: true,
    sessionId: 'visual-artifact-surfaces',
  });
  configure?.(store);
  return store;
}

function StoreSurface({
  configure,
  children,
}: {
  configure?: (store: AppStoreApi) => void;
  children: React.ReactNode;
}): React.ReactElement {
  const [store] = useState(() => createStoryStore(configure));
  return (
    <AppStoreContext.Provider value={store}>
      {children}
    </AppStoreContext.Provider>
  );
}

function artifactView(
  artifact: OpenArtifactView['artifact'],
  summary: ArtifactSummary,
  options: {
    cursor?: number;
    expanded?: Record<number, boolean>;
    error?: string;
    specType?: 'feature' | 'bugfix';
  } = {}
): OpenArtifactView {
  return {
    featureName,
    artifact,
    summary,
    source: 'Visual story source is represented by the parsed summary.',
    cursor: options.cursor ?? 0,
    expanded: options.expanded ?? {},
    error: options.error ? { message: options.error } : null,
    workflow: {
      workflowType: 'requirements-first',
      specType: options.specType ?? 'feature',
    },
  };
}

function ArtifactPanel({
  view,
  comments = {},
}: {
  view: OpenArtifactView;
  comments?: Record<string, ReviewAction[]>;
}): React.ReactElement {
  return (
    <StoreSurface
      configure={(store) =>
        store.setState({
          artifactViewOpen: view,
          specReviewComments: comments,
        })
      }
    >
      <ArtifactView />
    </StoreSurface>
  );
}

function generationEntry(
  artifact: ArtifactGenerationEntry['artifact'],
  name: string,
  summary: ArtifactSummary | null,
  options: { complete?: boolean; parseError?: string } = {}
): ArtifactGenerationEntry {
  return {
    absolutePath: `/workspace/.kiro/specs/${name}/${artifact}.md`,
    featureName: name,
    artifact,
    summary,
    lastWriteTs: Date.parse('2026-08-19T12:00:00.000Z'),
    complete: options.complete ?? false,
    parseError: options.parseError ?? null,
  };
}

function GenerationCard({
  entry,
}: {
  entry: ArtifactGenerationEntry;
}): React.ReactElement {
  return (
    <StoreSurface
      configure={(store) => store.setState({ artifactGenerating: entry })}
    >
      <ArtifactGenerationCard />
    </StoreSurface>
  );
}

function GenerationGallery(): React.ReactElement {
  return (
    <Box flexDirection="column">
      <GenerationCard
        entry={generationEntry('requirements', 'access-control', null)}
      />
      <GenerationCard
        entry={generationEntry(
          'requirements',
          'release-certification',
          requirementsSummary
        )}
      />
      <GenerationCard
        entry={generationEntry('design', 'binary-reuse', designSummary, {
          complete: true,
        })}
      />
      <GenerationCard
        entry={generationEntry('tasks', 'parallel-lanes', tasksSummary, {
          complete: true,
        })}
      />
      <GenerationCard
        entry={generationEntry('bugfix', 'aggregate-status', bugfixSummary, {
          parseError: 'incomplete clause while streaming',
        })}
      />
    </Box>
  );
}

const reviewLines = [
  '# Requirements',
  '',
  '### Requirement 1: Fast certification feedback',
  '',
  '1. WHEN a lane fails THEN the report SHALL name the failing lane.',
  '2. WHEN a retry succeeds THEN the report SHALL show the recovered result.',
  '',
  '### Requirement 2: Actionable evidence',
  '',
  '1. WHEN certification completes THEN the report SHALL link evidence.',
];

function ReviewSurface({
  error = null,
}: {
  error?: string | null;
}): React.ReactElement {
  return (
    <StoreSurface
      configure={(store) =>
        store.setState({
          specReviewView: {
            featureName,
            document: 'requirements',
            lines: error ? [] : reviewLines,
            cursor: error
              ? { lineIndex: 0, commentId: null }
              : {
                  lineIndex: 4,
                  commentId: stagedRequirementComment.id,
                },
            composing: null,
            error,
          },
          specReviewComments: error
            ? {}
            : {
                [`${featureName}/requirements`]: [stagedRequirementComment],
              },
        })
      }
    >
      <SpecReviewScreen />
    </StoreSurface>
  );
}

function CheckpointGallery(): React.ReactElement {
  return (
    <Box flexDirection="column">
      <StoreSurface
        configure={(store) =>
          store.setState({
            specPhaseCheckpoint: {
              featureName,
              phase: 'requirements',
              artifactPath: `/workspace/.kiro/specs/${featureName}/requirements.md`,
            },
          })
        }
      >
        <SpecCheckpointChip questionVisible />
      </StoreSurface>
      <StoreSurface
        configure={(store) =>
          store.setState({
            specPhaseCheckpoint: {
              featureName,
              phase: 'design',
              artifactPath: `/workspace/.kiro/specs/${featureName}/design.md`,
            },
            specReviewComments: {
              [`${featureName}/design`]: [stagedDesignComment],
            },
          })
        }
      >
        <SpecCheckpointChip questionVisible />
      </StoreSurface>
    </Box>
  );
}

function certification(
  readyText: string,
  assertions: StorybookAssertions,
  coversVisualStates: readonly string[]
): StorybookParameters {
  return {
    layout: 'fullscreen',
    experience: 'tui',
    coversVisualStates,
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport,
      assertions: {
        visible: assertions.visible ?? [readyText],
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
      },
    },
  };
}

const meta = {
  title: 'Compositions/ArtifactSurfaces',
  component: ArtifactView,
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    storyOrder: [
      'RequirementsArtifact',
      'DesignArtifact',
      'TasksArtifact',
      'BugfixArtifact',
      'EmptyArtifact',
      'ErrorArtifact',
      'ArtifactGeneration',
      'SpecReview',
      'SpecReviewError',
      'SpecCheckpoint',
      'SpecDescription',
    ],
    visualStates: {
      'artifact-requirements': {
        label: 'Requirements summary with selected item and staged comments',
      },
      'artifact-design': {
        label: 'Design overview and section summary',
      },
      'artifact-tasks': {
        label: 'Task summary with completion and expanded sub-tasks',
      },
      'artifact-bugfix': {
        label: 'Bug analysis with expanded behavioral clauses',
      },
      'artifact-empty': {
        label: 'Artifact with no parsed summary items',
      },
      'artifact-error': {
        label: 'Artifact load failure',
      },
      'generation-loading': {
        label: 'Artifact generation before the first successful parse',
      },
      'generation-writing': {
        label: 'Artifact generation with a live parsed summary',
      },
      'generation-complete': {
        label: 'Completed artifact generation summaries',
      },
      'generation-parse-error': {
        label: 'Non-blocking parse failure during generation',
      },
      'review-document': {
        label: 'Spec review document with line cursor',
      },
      'review-staged-comment': {
        label: 'Spec review document with an anchored staged comment',
      },
      'review-error': {
        label: 'Spec review document load failure',
      },
      'checkpoint-ready': {
        label: 'Completed spec phase ready for review',
      },
      'checkpoint-staged': {
        label: 'Completed spec phase with staged feedback',
      },
      'description-intro': {
        label: 'Spec description collection prompt',
      },
      'artifact-filesystem-switch': {
        label: 'Switching artifact documents through production loaders',
        gapType: 'integration-only',
        description:
          'Document switching requires a real workspace-backed spec fixture.',
      },
      'artifact-open-review': {
        label: 'Opening the selected artifact item in review',
        gapType: 'integration-only',
        description:
          'Opening review resolves the selected source slice from the workspace.',
      },
      'generation-event-transition': {
        label: 'Live write events transitioning generation to completion',
        gapType: 'integration-only',
        description:
          'The event-driven transition is owned by ACP and filesystem integration tests.',
      },
      'review-mouse-positioning': {
        label: 'Mouse positioning and double-click comment editing',
        gapType: 'integration-only',
        description:
          'Mouse capture and terminal coordinates require the PTY integration harness.',
      },
    },
  },
};

export default meta;

export const RequirementsArtifact = {
  render: () => (
    <ArtifactPanel
      view={artifactView('requirements', requirementsSummary)}
      comments={{
        [`${featureName}/requirements`]: [stagedRequirementComment],
        [`${featureName}/design`]: [stagedDesignComment],
      }}
    />
  ),
  parameters: certification(
    `/spec view ${featureName} requirements`,
    {
      visible: [
        `/spec view ${featureName} requirements`,
        'Requirements',
        'Design',
        'Tasks',
        'Requirement 1: Fast certification feedback',
        'As a contributor, I want failed lanes identified quickly.',
        'Requirement 2: Actionable evidence',
        '(no user story)',
        'send 1 comment',
      ],
      ordered: [
        'Requirements',
        'Design',
        'Tasks',
        'Requirement 1: Fast certification feedback',
        'Requirement 2: Actionable evidence',
      ],
      occurrences: {
        'Requirement 1: Fast certification feedback': 1,
        'Requirement 2: Actionable evidence': 1,
      },
    },
    ['artifact-requirements']
  ),
};

export const DesignArtifact = {
  render: () => (
    <ArtifactPanel
      view={artifactView('design', designSummary, { cursor: 1 })}
    />
  ),
  parameters: certification(
    'Artifact contract',
    {
      visible: [
        `/spec view ${featureName} design`,
        'Overview',
        'Build once per platform',
        'Architecture',
        'Artifact contract',
        'Failure isolation',
      ],
      ordered: [
        'Overview',
        'Build once per platform',
        'Architecture',
        'Artifact contract',
        'Failure isolation',
      ],
    },
    ['artifact-design']
  ),
};

export const TasksArtifact = {
  render: () => (
    <ArtifactPanel
      view={artifactView('tasks', tasksSummary, {
        cursor: 1,
        expanded: { 1: true },
      })}
    />
  ),
  parameters: certification(
    'Publish visual evidence',
    {
      visible: [
        `/spec view ${featureName} tasks`,
        '1. Build platform assets',
        '2. Run verification lanes',
        'Run deterministic smoke tests',
        'Publish visual evidence',
        'expand',
      ],
      ordered: [
        '1. Build platform assets',
        '2. Run verification lanes',
        'Run deterministic smoke tests',
        'Publish visual evidence',
      ],
      occurrences: {
        '2. Run verification lanes': 1,
        'Publish visual evidence': 1,
      },
    },
    ['artifact-tasks']
  ),
};

export const BugfixArtifact = {
  render: () => (
    <ArtifactPanel
      view={artifactView('bugfix', bugfixSummary, {
        cursor: 1,
        expanded: { 1: true },
        specType: 'bugfix',
      })}
    />
  ),
  parameters: certification(
    '2.2 WHEN the lane is retried successfully',
    {
      visible: [
        `/spec view ${featureName} bugfix`,
        'Bug analysis',
        'The bug',
        'aggregate certification status pending',
        'Current Behavior (Defect)',
        'Expected Behavior',
        '2.1 WHEN a child lane fails',
        '2.2 WHEN the lane is retried successfully',
      ],
      ordered: [
        'The bug',
        'Current Behavior (Defect)',
        'Expected Behavior',
        '2.1 WHEN a child lane fails',
        '2.2 WHEN the lane is retried successfully',
      ],
    },
    ['artifact-bugfix']
  ),
};

export const EmptyArtifact = {
  render: () => (
    <ArtifactPanel view={artifactView('requirements', emptyRequirements)} />
  ),
  parameters: certification(
    'No requirements found.',
    {
      visible: [
        `/spec view ${featureName} requirements`,
        'No requirements found.',
        'Look for blocks beginning with "### Requirement N:".',
      ],
      hidden: ['Requirement 1:'],
    },
    ['artifact-empty']
  ),
};

export const ErrorArtifact = {
  render: () => (
    <ArtifactPanel
      view={artifactView('tasks', tasksSummary, {
        error: `No artifact found at .kiro/specs/${featureName}/tasks.md`,
      })}
    />
  ),
  parameters: certification(
    'No artifact found',
    {
      visible: [
        `/spec view ${featureName} tasks`,
        `No artifact found at .kiro/specs/${featureName}/tasks.md`,
        'esc',
        'close',
      ],
      hidden: ['Run verification lanes'],
    },
    ['artifact-error']
  ),
};

export const ArtifactGeneration = {
  render: () => <GenerationGallery />,
  parameters: certification(
    'aggregate-status',
    {
      visible: [
        'Requirements',
        'access-control',
        '(loading',
        'release-certification',
        '2 requirements',
        'Design',
        'binary-reuse',
        'Build once per platform',
        '3 sections',
        'Tasks',
        'parallel-lanes',
        '2 tasks',
        '1/2 done',
        '4 sub-tasks',
        'Bug analysis',
        'aggregate-status',
        'parse failed',
        '2 sections, 3 clauses',
      ],
      ordered: [
        'access-control',
        'release-certification',
        'binary-reuse',
        'parallel-lanes',
        'aggregate-status',
      ],
      occurrences: {
        'access-control': 1,
        'release-certification': 1,
        'binary-reuse': 1,
        'parallel-lanes': 1,
        'aggregate-status': 1,
      },
    },
    [
      'generation-loading',
      'generation-writing',
      'generation-complete',
      'generation-parse-error',
    ]
  ),
};

export const SpecReview = {
  render: () => <ReviewSurface />,
  parameters: certification(
    'Name the failed platform as well as the lane.',
    {
      visible: [
        'Review requirements.md',
        featureName,
        '1 comment staged',
        'Requirement 1: Fast certification feedback',
        'WHEN a lane fails THEN the report SHALL name the failing lane.',
        'Name the failed platform as well as the lane.',
        'to edit',
        'to delete',
      ],
      ordered: [
        'Review requirements.md',
        'Requirement 1: Fast certification feedback',
        'WHEN a lane fails THEN the report SHALL name the failing lane.',
        'Name the failed platform as well as the lane.',
      ],
      occurrences: {
        'Name the failed platform as well as the lane.': 1,
      },
    },
    ['review-document', 'review-staged-comment']
  ),
};

export const SpecReviewError = {
  render: () => (
    <ReviewSurface error="Unable to read requirements.md: permission denied" />
  ),
  parameters: certification(
    'Unable to read requirements.md',
    {
      visible: [
        'Review requirements.md',
        featureName,
        'Unable to read requirements.md: permission denied',
        'to go back',
      ],
      hidden: ['comment staged'],
    },
    ['review-error']
  ),
};

export const SpecCheckpoint = {
  render: () => <CheckpointGallery />,
  parameters: certification(
    'Design complete',
    {
      visible: [
        'Requirements complete',
        'Design complete',
        'ctrl+X',
        'to review',
        '1 comment staged',
      ],
      ordered: ['Requirements complete', 'Design complete'],
      occurrences: {
        'Requirements complete': 1,
        'Design complete': 1,
        'ctrl+X': 2,
        'to review': 2,
      },
    },
    ['checkpoint-ready', 'checkpoint-staged']
  ),
};

export const SpecDescription = {
  render: () => <SpecDescriptionIntro featureName="binary reuse" />,
  parameters: certification(
    'What should this spec cover?',
    {
      visible: [
        'Starting spec: "binary reuse"',
        'What should this spec cover?',
        'Describe it in a sentence or two.',
      ],
      ordered: [
        'Starting spec: "binary reuse"',
        'What should this spec cover?',
        'Describe it in a sentence or two.',
      ],
      occurrences: {
        'Starting spec: "binary reuse"': 1,
        'What should this spec cover?': 1,
      },
    },
    ['description-intro']
  ),
};
