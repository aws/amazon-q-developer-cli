import React, { useState } from 'react';
import { Box } from '../../renderer.js';
import type { SourceProviderResource } from '@kiro/acp-type-covenant';
import { BlockingErrorAlert } from '../ui/alert/BlockingErrorAlert.js';
import { CloudQuitPrompt, CLOUD_QUIT_TURN_OFF } from '../ui/CloudQuitPrompt.js';
import {
  Explorer,
  type ExplorerColumn,
  type ExplorerRow,
} from '../ui/Explorer.js';
import { ErrorBoundary } from '../ui/ErrorBoundary.js';
import { KiroEyes } from '../ui/KiroEyes.js';
import { Question } from '../ui/Question.js';
import { RepoPickerPanel } from '../ui/RepoPickerPanel.js';
import {
  SessionPickerPanel,
  type SessionPickerRow,
} from '../ui/SessionPickerPanel.js';
import { SourceProviderGate } from '../ui/SourceProviderGate.js';
import { SurveyPromptBar } from '../ui/SurveyPromptBar.js';
import { ThinkingIndicator } from '../ui/ThinkingIndicator.js';
import { TrustAllToolsBanner } from '../ui/TrustAllToolsBanner.js';
import { TrustAllToolsGate } from '../ui/TrustAllToolsGate.js';
import { VirtualScrollList } from '../ui/VirtualScrollList.js';
import { VoiceModelDownloadGate } from '../ui/VoiceModelDownloadGate.js';
import { Text } from '../ui/text/Text.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../storybook/contracts.js';
import { certifyVisualStory } from '../../storybook/story-certification.js';

const viewport = { columns: 120, rows: 38 };

const meta = {
  title: 'Compositions/InteractionSurfaces',
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    visualStates: {
      'source-provider-local': {
        label: 'Local source-provider gate with browser setup option',
      },
      'source-provider-checking': {
        label: 'Source-provider retry in progress',
      },
      'source-provider-retry-complete': {
        label: 'Source-provider retry returns to the unresolved gate',
      },
      'source-provider-remote': {
        label: 'Remote source-provider gate without browser option',
      },
      'trust-gate': {
        label: 'Trust-all warning and decision menu',
      },
      'trust-accepted': {
        label: 'Trust-all decision resolved for the current session',
      },
      'question-options': {
        label: 'Question with recommended and multi-select options',
      },
      'question-suboptions': {
        label: 'Question multi-select detail page',
      },
      'question-resolved': {
        label: 'Question submitted through the production input handler',
      },
      'cloud-quit': {
        label: 'Cloud detach or stop decision',
      },
      'voice-download': {
        label: 'First-use voice model download gate',
      },
      'repo-picker': {
        label: 'Repository picker with selected and available sources',
      },
      'repo-picker-updated': {
        label: 'Repository selection changed with keyboard input',
      },
      'session-picker': {
        label: 'Mixed local and cloud session picker',
      },
      'session-selected': {
        label: 'Session selection resolved through the keyboard handler',
      },
      explorer: {
        label: 'Searchable explorer with contextual preview',
      },
      'explorer-selected': {
        label: 'Explorer row selected after navigation',
      },
      'utility-surfaces': {
        label: 'Blocking alert, thinking, survey, trust banner, and list',
      },
      'error-boundary-fallback': {
        label: 'Error boundary fallback after an uncaught process error',
        gapType: 'integration-only',
        description:
          'Triggering a process-level uncaught exception safely belongs to the terminal integration lane.',
      },
    },
    storyOrder: [
      'SourceProviderRetryJourney',
      'RemoteSourceProvider',
      'TrustAllToolsJourney',
      'QuestionMultiSelectJourney',
      'CloudQuitJourney',
      'VoiceDownloadJourney',
      'RepositoryPickerJourney',
      'SessionPickerJourney',
      'ExplorerJourney',
      'UtilitySurfaces',
    ],
  },
};

export default meta;

function journey(
  readyText: string,
  captures: NonNullable<
    NonNullable<StorybookParameters['certification']>['captures']
  >,
  rows = viewport.rows
): StorybookParameters {
  return {
    layout: 'fullscreen',
    capturesKeyboard: true,
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport: { ...viewport, rows },
      assertions: { hidden: ['undefined'] },
      captures,
    },
  };
}

function SourceProviderRetryStory(): React.ReactElement {
  const [status, setStatus] = useState('idle');

  return (
    <Box flexDirection="column">
      <SourceProviderGate
        setupUrl="https://app.kiro.dev/settings/source-providers"
        isRemote={false}
        onOpenBrowser={() => setStatus('browser-opened')}
        onRetry={async () => {
          setStatus('checking');
          await new Promise((resolve) => setTimeout(resolve, 300));
          setStatus('retry-complete');
        }}
        onQuit={() => setStatus('quit')}
      />
      <Text>{`Contract: source-provider=${status}`}</Text>
    </Box>
  );
}

const sourceProviderPlay: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('local-gate');
  await press('down');
  await press('enter');
  await waitFor('Connecting');
  await capture('checking');
  await waitFor('Contract: source-provider=retry-complete');
  await capture('retry-complete');
};

export const SourceProviderRetryJourney = {
  render: SourceProviderRetryStory,
  parameters: journey('Source provider not found', {
    'local-gate': {
      label: 'local gate before retry',
      coversVisualStates: ['source-provider-local'],
      assertions: {
        visible: [
          'Source provider not found',
          'Open in browser',
          'Refresh and try again',
          'Quit',
          'Contract: source-provider=idle',
        ],
        hidden: ['Connecting'],
      },
    },
    checking: {
      label: 'provider retry in progress',
      coversVisualStates: ['source-provider-checking'],
      assertions: {
        visible: ['Source provider not found', 'Connecting'],
        hidden: ['Open in browser', 'Refresh and try again'],
      },
    },
    'retry-complete': {
      label: 'provider retry returned to the unresolved gate',
      coversVisualStates: ['source-provider-retry-complete'],
      assertions: {
        visible: [
          'Open in browser',
          'Refresh and try again',
          'Contract: source-provider=retry-complete',
        ],
        hidden: ['Connecting'],
      },
    },
  }),
  play: sourceProviderPlay,
};

export const RemoteSourceProvider = {
  render: () => (
    <SourceProviderGate
      setupUrl={null}
      isRemote
      onOpenBrowser={() => undefined}
      onRetry={() => undefined}
      onQuit={() => undefined}
    />
  ),
  parameters: certifyVisualStory(
    'Source provider not found',
    {
      visible: [
        'Source provider not found',
        'Open the URL above on any device',
        'Refresh and try again',
        'Quit',
      ],
      hidden: ['Open in browser'],
    },
    ['source-provider-remote'],
    viewport
  ),
};

function TrustAllToolsStory(): React.ReactElement {
  const [result, setResult] = useState('pending');
  return (
    <Box flexDirection="column">
      <TrustAllToolsGate
        onAccept={() => setResult('accepted-once')}
        onAcceptAlways={() => setResult('accepted-always')}
        onExit={() => setResult('exit')}
      />
      <Text>{`Contract: trust=${result}`}</Text>
    </Box>
  );
}

const trustAllPlay: StorybookPlay = async ({ press, waitFor, capture }) => {
  await capture('gate');
  await press('down');
  await press('enter');
  await waitFor('Contract: trust=accepted-once');
  await capture('accepted');
};

export const TrustAllToolsJourney = {
  render: TrustAllToolsStory,
  parameters: journey(
    'Warning: Kiro is running in trust all tools mode',
    {
      gate: {
        label: 'trust-all decision pending',
        coversVisualStates: ['trust-gate'],
        assertions: {
          visible: [
            'Warning: Kiro is running in trust all tools mode',
            'No, exit',
            'Yes, I accept',
            "Yes, and don't ask again",
            'Contract: trust=pending',
          ],
        },
      },
      accepted: {
        label: 'trust accepted for the current session',
        coversVisualStates: ['trust-accepted'],
        assertions: {
          visible: ['Contract: trust=accepted-once'],
          occurrences: { 'Contract: trust=accepted-once': 1 },
        },
      },
    },
    48
  ),
  play: trustAllPlay,
};

const questionOptions = [
  {
    title: 'Keep the current API',
    description: 'Preserves compatibility for existing integrations.',
    recommended: true,
  },
  {
    title: 'Change selected surfaces',
    description: 'Choose the affected areas.',
    subOptionsLabel: 'Affected areas',
    subOptions: [
      { title: 'Runtime', description: 'Update runtime behavior.' },
      { title: 'Tests', description: 'Update verification behavior.' },
    ],
  },
];

function QuestionStory(): React.ReactElement {
  const [answer, setAnswer] = useState('pending');
  return (
    <Box flexDirection="column">
      <Question
        question="**Which compatibility strategy should this release use?**"
        options={questionOptions}
        onAnswer={(value) => {
          setAnswer(value);
          return true;
        }}
        onCancel={() => setAnswer('cancelled')}
      />
      <Text>{`Contract: answer=${answer}`}</Text>
    </Box>
  );
}

const questionPlay: StorybookPlay = async ({
  press,
  type,
  waitFor,
  capture,
}) => {
  await capture('options');
  await press('down');
  await press('enter');
  await waitFor('Affected areas');
  await capture('suboptions');
  await type(' ');
  await press('enter');
  await waitFor('Contract: answer=Change selected surfaces [Tests]');
  await capture('resolved');
};

export const QuestionMultiSelectJourney = {
  render: QuestionStory,
  parameters: journey('Which compatibility strategy', {
    options: {
      label: 'question options with a recommended choice',
      coversVisualStates: ['question-options'],
      assertions: {
        visible: [
          'Which compatibility strategy should this release use?',
          'Keep the current API',
          '(recommended)',
          'Change selected surfaces',
          'Type a different answer',
          'Contract: answer=pending',
        ],
      },
    },
    suboptions: {
      label: 'multi-select detail page',
      coversVisualStates: ['question-suboptions'],
      assertions: {
        visible: ['Affected areas', 'Runtime', 'Tests', 'space to toggle'],
        hidden: ['Type a different answer'],
      },
    },
    resolved: {
      label: 'multi-select answer submitted',
      coversVisualStates: ['question-resolved'],
      assertions: {
        visible: ['Contract: answer=Change selected surfaces [Tests]'],
      },
    },
  }),
  play: questionPlay,
};

function CloudQuitStory(): React.ReactElement {
  const [result, setResult] = useState('pending');
  return (
    <Box flexDirection="column">
      <CloudQuitPrompt
        onKeepRunning={() => setResult('keep-running')}
        onTurnOff={() => setResult('turn-off')}
        onCancel={() => setResult('cancel')}
      />
      <Text>{`Contract: cloud-quit=${result}`}</Text>
    </Box>
  );
}

const cloudQuitPlay: StorybookPlay = async ({ press, waitFor, capture }) => {
  await capture('pending');
  await press('down');
  await press('enter');
  await waitFor('Contract: cloud-quit=turn-off');
  await capture('resolved');
};

export const CloudQuitJourney = {
  render: CloudQuitStory,
  parameters: journey('Would you like the agent to continue working?', {
    pending: {
      label: 'cloud quit decision pending',
      coversVisualStates: ['cloud-quit'],
      assertions: {
        visible: [
          'Would you like the agent to continue working?',
          'Yes (agent continues)',
          CLOUD_QUIT_TURN_OFF,
          'Contract: cloud-quit=pending',
        ],
      },
    },
    resolved: {
      label: 'cloud agent stop selected',
      assertions: {
        visible: ['Contract: cloud-quit=turn-off'],
      },
    },
  }),
  play: cloudQuitPlay,
};

function VoiceDownloadStory(): React.ReactElement {
  const [result, setResult] = useState('pending');
  return (
    <Box flexDirection="column">
      <VoiceModelDownloadGate
        info={{
          model: 'whisper-small',
          sizeMb: 466,
          license: 'MIT',
          licenseUrl: 'https://example.com/whisper-license',
        }}
        onConfirm={() => setResult('confirmed')}
        onDecline={() => setResult('declined')}
      />
      <Text>{`Contract: voice=${result}`}</Text>
    </Box>
  );
}

const voicePlay: StorybookPlay = async ({ type, waitFor, capture }) => {
  await capture('pending');
  await type('y');
  await waitFor('Contract: voice=confirmed');
  await capture('confirmed');
};

export const VoiceDownloadJourney = {
  render: VoiceDownloadStory,
  parameters: journey('Voice setup', {
    pending: {
      label: 'voice model download confirmation',
      coversVisualStates: ['voice-download'],
      assertions: {
        visible: [
          'Voice setup',
          'whisper-small',
          '466MB',
          'MIT license',
          'Yes, download the model',
          'No, not now',
        ],
      },
    },
    confirmed: {
      label: 'voice model download confirmed with y shortcut',
      assertions: { visible: ['Contract: voice=confirmed'] },
    },
  }),
  play: voicePlay,
};

const repositories: SourceProviderResource[] = [
  {
    name: 'kiro-cli',
    providerType: 'GITHUB',
    defaultBranch: 'main',
    updatedAt: '2026-08-18T12:00:00.000Z',
  },
  {
    name: 'terminal-renderer',
    providerType: 'GITHUB',
    defaultBranch: 'trunk',
    updatedAt: '2026-08-17T12:00:00.000Z',
  },
] as SourceProviderResource[];

function RepositoryPickerStory(): React.ReactElement {
  const [result, setResult] = useState('pending');
  const [open, setOpen] = useState(true);
  return (
    <Box flexDirection="column">
      {open ? (
        <RepoPickerPanel
          resources={repositories}
          initialSelected={['kiro-cli']}
          onSubmit={(selected) => setResult(selected.join(','))}
          onClose={() => setOpen(false)}
        />
      ) : null}
      <Text>{`Contract: repos=${result}`}</Text>
    </Box>
  );
}

const repositoryPlay: StorybookPlay = async ({
  press,
  type,
  waitFor,
  capture,
}) => {
  await capture('picker');
  await press('down');
  await type(' ');
  await waitFor('Selected(2)');
  await capture('updated');
  await press('escape');
  await waitFor('Contract: repos=kiro-cli,terminal-renderer');
  await capture('saved');
};

export const RepositoryPickerJourney = {
  render: RepositoryPickerStory,
  parameters: journey('/repo', {
    picker: {
      label: 'repository picker with one existing selection',
      coversVisualStates: ['repo-picker'],
      assertions: {
        visible: [
          'Selected(1)',
          'kiro-cli',
          'terminal-renderer',
          'GITHUB',
          'space to toggle',
        ],
      },
    },
    updated: {
      label: 'second repository selected',
      coversVisualStates: ['repo-picker-updated'],
      assertions: {
        visible: ['Selected(2)', 'kiro-cli', 'terminal-renderer'],
      },
    },
    saved: {
      label: 'repository selection saved on escape',
      assertions: {
        visible: ['Contract: repos=kiro-cli,terminal-renderer'],
        hidden: ['Selected(2)'],
      },
    },
  }),
  play: repositoryPlay,
};

const sessions: SessionPickerRow[] = [
  {
    sessionId: '11111111-local-session',
    title: 'Fix terminal rendering',
    environment: 'local',
    status: 'idle',
    updatedAt: '2026-08-19T10:00:00.000Z',
  },
  {
    sessionId: '22222222-cloud-session',
    title: 'Run release verification',
    environment: 'cloud',
    status: 'working',
    updatedAt: '2026-08-19T11:00:00.000Z',
  },
];

function SessionPickerStory(): React.ReactElement {
  const [result, setResult] = useState('pending');
  const [open, setOpen] = useState(true);
  return (
    <Box flexDirection="column">
      {open ? (
        <SessionPickerPanel
          rows={sessions}
          onSelect={(id, environment) =>
            setResult(`${id.slice(0, 8)}:${environment}`)
          }
          onClose={() => setOpen(false)}
        />
      ) : null}
      <Text>{`Contract: session=${result}`}</Text>
    </Box>
  );
}

const sessionPlay: StorybookPlay = async ({ press, waitFor, capture }) => {
  await capture('picker');
  await press('down');
  await press('enter');
  await waitFor('Contract: session=22222222:cloud');
  await capture('selected');
};

export const SessionPickerJourney = {
  render: SessionPickerStory,
  parameters: journey('/sessions', {
    picker: {
      label: 'mixed local and cloud session picker',
      coversVisualStates: ['session-picker'],
      assertions: {
        visible: [
          'ID',
          'Name',
          'Environment',
          'Status',
          'Fix terminal rendering',
          'Run release verification',
          'local',
          'cloud',
        ],
      },
    },
    selected: {
      label: 'cloud session selected',
      coversVisualStates: ['session-selected'],
      assertions: {
        visible: ['Contract: session=22222222:cloud'],
        hidden: ['Fix terminal rendering'],
      },
    },
  }),
  play: sessionPlay,
};

const explorerColumns: ExplorerColumn[] = [
  { key: 'name', label: 'Surface' },
  { key: 'status', label: 'Status' },
];
const explorerRows: ExplorerRow[] = [
  {
    id: 'transcript',
    values: { name: 'Transcript', status: 'covered' },
    preview: {
      heading: 'Transcript',
      body: 'User prompt\nThinking\nTool call\nAssistant response',
    },
  },
  {
    id: 'markdown',
    values: { name: 'Markdown', status: 'covered' },
    preview: {
      heading: 'Markdown',
      body: 'Heading\nList\nTable\nCode fence',
    },
  },
];

function ExplorerStory(): React.ReactElement {
  const [result, setResult] = useState('pending');
  return (
    <Box flexDirection="column">
      <Explorer
        title="Visual surface explorer"
        description="Inspect certified component groups."
        columns={explorerColumns}
        rows={explorerRows}
        previewHeading="Evidence"
        onSelect={(row) => setResult(row.id)}
        onClose={() => setResult('closed')}
      />
      <Text>{`Contract: explorer=${result}`}</Text>
    </Box>
  );
}

const explorerPlay: StorybookPlay = async ({ press, waitFor, capture }) => {
  await capture('open');
  await press('down');
  await waitFor('Code fence');
  await capture('markdown-preview');
  await press('enter');
  await waitFor('Contract: explorer=markdown');
  await capture('selected');
};

export const ExplorerJourney = {
  render: ExplorerStory,
  parameters: journey('Visual surface explorer', {
    open: {
      label: 'explorer showing the first contextual preview',
      coversVisualStates: ['explorer'],
      assertions: {
        visible: [
          'Visual surface explorer',
          'Inspect certified component groups',
          'Transcript',
          'Markdown',
          'User prompt',
          'Assistant response',
        ],
      },
    },
    'markdown-preview': {
      label: 'explorer preview follows keyboard navigation',
      assertions: {
        visible: ['Markdown', 'Heading', 'List', 'Table', 'Code fence'],
        hidden: ['User prompt'],
      },
    },
    selected: {
      label: 'explorer selection callback invoked',
      coversVisualStates: ['explorer-selected'],
      assertions: { visible: ['Contract: explorer=markdown'] },
    },
  }),
  play: explorerPlay,
};

function UtilitySurfaceStory(): React.ReactElement {
  return (
    <ErrorBoundary>
      <Box flexDirection="column">
        <BlockingErrorAlert
          message="Verification is blocked"
          guidance="Resolve the failing visual frame before continuing."
        />
        <ThinkingIndicator />
        <KiroEyes message="Awaiting model response" />
        <SurveyPromptBar message="How was this verification run?" />
        <TrustAllToolsBanner />
        <VirtualScrollList
          items={['Transcript captured', 'Markdown captured']}
          height={4}
          renderItem={(item) => <Text>{item}</Text>}
        />
        <Text>Boundary healthy</Text>
      </Box>
    </ErrorBoundary>
  );
}

export const UtilitySurfaces = {
  render: UtilitySurfaceStory,
  parameters: certifyVisualStory(
    'Verification is blocked',
    {
      visible: [
        'Verification is blocked',
        'Resolve the failing visual frame before continuing',
        'Thinking...',
        'Awaiting model response',
        'How was this verification run?',
        'ctrl+y',
        'Trust All Tools active',
        'Transcript captured',
        'Markdown captured',
        'Boundary healthy',
      ],
      ordered: [
        'Verification is blocked',
        'Thinking...',
        'Awaiting model response',
        'How was this verification run?',
        'Trust All Tools active',
        'Transcript captured',
        'Markdown captured',
        'Boundary healthy',
      ],
    },
    ['utility-surfaces'],
    viewport
  ),
};
