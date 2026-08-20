import React, { useState } from 'react';
import { Box } from './../../renderer.js';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  type AppStoreApi,
} from '../../stores/app-store.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../storybook/contracts.js';
import {
  AgentEventType,
  ApprovalOptionId,
  type ApprovalRequestInfo,
  type PermissionResponse,
} from '../../types/agent-events.js';
import { Text } from './text/Text.js';
import { ToolUseMessage } from './ToolUseMessage.js';
import { ApprovalRequest } from './ApprovalRequest.js';

type ApprovalScenario =
  | 'allow'
  | 'reject'
  | 'permission-first'
  | 'kas-trust-scope';

interface ApprovalStoryProps {
  scenario: ApprovalScenario;
}

const viewport = { columns: 120, rows: 28 };
const shellCommand = 'git status --short';

const allowOnce = {
  kind: ApprovalOptionId.AllowOnce,
  name: 'Allow once',
  optionId: ApprovalOptionId.AllowOnce,
};
const rejectOnce = {
  kind: ApprovalOptionId.RejectOnce,
  name: 'Reject once',
  optionId: ApprovalOptionId.RejectOnce,
};

function approvalFor(
  scenario: ApprovalScenario,
  resolve: (response: PermissionResponse) => void
): ApprovalRequestInfo {
  if (scenario === 'permission-first') {
    return {
      toolId: 'fs_write',
      toolCall: {
        toolCallId: 'permission-first-write',
        title: 'Creating report.ts',
        name: 'fs_write',
        kind: 'edit',
        origin: 'builtin',
        rawInput: {
          command: 'str_replace',
          path: '/workspace/report.ts',
          oldStr: 'export const ready = false;',
          newStr: 'export const ready = true;',
        },
      },
      permissionOptions: [allowOnce, rejectOnce],
      resolve,
    };
  }

  if (scenario === 'kas-trust-scope') {
    return {
      originSessionId: 'main-session',
      toolId: 'execute_bash',
      toolCall: {
        toolCallId: 'kas-shell',
        title: 'execute_bash',
        name: 'execute_bash',
        kind: 'execute',
        origin: 'builtin',
        rawInput: {
          command: 'git status && echo release',
          cwd: '/workspace',
        },
      },
      permissionOptions: [
        {
          kind: ApprovalOptionId.AllowOnce,
          name: 'Allow once',
          optionId: 'accept-once',
        },
        {
          kind: ApprovalOptionId.AllowAlways,
          name: 'Always allow',
          optionId: 'always-accept',
        },
        {
          kind: ApprovalOptionId.RejectOnce,
          name: 'Reject once',
          optionId: 'reject-once',
        },
      ],
      consentContext: {
        capability: 'shell',
        resource: 'git status && echo release',
        triggeringResource: 'git status',
        workspaceRoot: '/workspace',
      },
      resolve,
    };
  }

  return {
    toolId: 'execute_bash',
    toolCall: {
      toolCallId: `${scenario}-shell`,
      title: 'execute_bash',
      name: 'execute_bash',
      kind: 'execute',
      origin: 'builtin',
      rawInput: { command: shellCommand, cwd: '/workspace' },
    },
    permissionOptions: [allowOnce, rejectOnce],
    resolve,
  };
}

function createApprovalStore(
  scenario: ApprovalScenario,
  onResolve: (response: PermissionResponse) => void
): AppStoreApi {
  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine: scenario === 'kas-trust-scope' ? 'kas' : 'v2',
  });
  store.setState({ sessionId: 'main-session', isInitialized: true });
  const handleEvent = store.getState().createStreamEventHandler();
  const approval = approvalFor(scenario, onResolve);

  if (scenario !== 'permission-first') {
    const id = approval.toolCall.toolCallId;
    const command =
      scenario === 'kas-trust-scope'
        ? 'git status && echo release'
        : shellCommand;
    handleEvent({
      type: AgentEventType.ToolCall,
      id,
      name: 'execute_bash',
      kind: 'execute',
      origin: 'builtin',
      args: { command, cwd: '/workspace' },
    });
  }
  handleEvent({ type: AgentEventType.ApprovalRequest, value: approval });
  return store;
}

function consentFact(response: PermissionResponse | null): string {
  if (!response || response.outcome !== 'selected') return '';
  const kiro = response._meta?.kiro;
  const consent =
    kiro && typeof kiro.consent === 'object' && kiro.consent !== null
      ? kiro.consent
      : null;
  const scope = consent?.scope;
  const resource = consent?.resource;
  return `${typeof scope === 'string' ? ` | scope=${scope}` : ''}${
    typeof resource === 'string' ? ` | resource=${resource}` : ''
  }`;
}

function ApprovalContract({
  resolution,
}: {
  resolution: PermissionResponse | null;
}): React.ReactElement {
  const store = React.useContext(AppStoreContext);
  if (!store) throw new Error('ApprovalContract requires AppStoreContext');
  const state = store.getState();
  const tool = state.messages.find(
    (message) => message.role === MessageRole.ToolUse
  );
  const toolState =
    tool?.role === MessageRole.ToolUse ? (tool.status ?? 'unmarked') : 'absent';
  const outcome =
    resolution?.outcome === 'selected'
      ? `selected ${resolution.optionId}`
      : (resolution?.outcome ?? 'pending');

  return (
    <Text>
      {`Contract: ${outcome} | queue=${state.approvalQueue.length} | tool=${toolState}${consentFact(resolution)}`}
    </Text>
  );
}

function ApprovalTranscript(): React.ReactElement | null {
  const store = React.useContext(AppStoreContext);
  if (!store) throw new Error('ApprovalTranscript requires AppStoreContext');
  const message = store
    .getState()
    .messages.find((candidate) => candidate.role === MessageRole.ToolUse);
  if (!message || message.role !== MessageRole.ToolUse) return null;
  return (
    <ToolUseMessage
      id={message.id}
      name={message.name}
      content={message.content}
      kind={message.kind}
      origin={message.origin}
      status={message.status}
      isFinished={message.isFinished}
      result={message.result}
    />
  );
}

function ApprovalStory({ scenario }: ApprovalStoryProps): React.ReactElement {
  const [resolution, setResolution] = useState<PermissionResponse | null>(null);
  const [store] = useState(() => createApprovalStore(scenario, setResolution));

  return (
    <AppStoreContext.Provider value={store}>
      <Box flexDirection="column">
        <Text>Approval history</Text>
        <ApprovalTranscript />
        <ApprovalRequest onDrillInSubmit={() => undefined} />
        <ApprovalContract resolution={resolution} />
      </Box>
    </AppStoreContext.Provider>
  );
}

function certification(
  readyText: string,
  captures: NonNullable<
    NonNullable<StorybookParameters['certification']>['captures']
  >
): StorybookParameters {
  return {
    layout: 'fullscreen',
    capturesKeyboard: true,
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport,
      assertions: {
        visible: ['Approval history'],
        hidden: ['undefined'],
      },
      captures,
    },
  };
}

const captureAllow: StorybookPlay = async ({ press, waitFor, capture }) => {
  await capture('pending');
  await press('enter');
  await waitFor('Contract: selected allow_once');
  await capture('allowed');
};

const captureReject: StorybookPlay = async ({ press, waitFor, capture }) => {
  await capture('pending');
  await press('down');
  await press('enter');
  await waitFor('Contract: selected reject_once');
  await capture('rejected');
};

const capturePermissionFirst: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('permission-first');
  await press('enter');
  await waitFor('Contract: selected allow_once');
  await capture('permission-first-allowed');
};

const captureKasTrustScope: StorybookPlay = async ({
  press,
  type,
  waitFor,
  capture,
}) => {
  await capture('pending');
  await press('down');
  await press('enter');
  await waitFor('trust [session]');
  await capture('trust-session');
  await type('s');
  await waitFor('trust [workspace]');
  await capture('trust-workspace');
  await press('enter');
  await waitFor('scope=workspace | resource=git status');
  await capture('trusted');
};

const meta = {
  title: 'UI/ApprovalRequest',
  component: ApprovalRequest,
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    visualStates: {
      'allow-pending': { label: 'Tool awaiting one-time approval' },
      allowed: { label: 'One-time approval resolved through the store' },
      'reject-pending': { label: 'Tool awaiting rejection decision' },
      rejected: { label: 'Rejected tool retained in history' },
      'permission-first': {
        label: 'Permission arrives before its ToolCall event',
      },
      'permission-first-allowed': {
        label: 'Permission-first request resolves without inventing a tool row',
      },
      'kas-trust-session': { label: 'KAS trust choices at session scope' },
      'kas-trust-workspace': { label: 'KAS trust choices at workspace scope' },
      'kas-trusted': {
        label: 'KAS exact-resource trust resolved with consent metadata',
      },
    },
    storyOrder: [
      'AllowOnceJourney',
      'RejectJourney',
      'PermissionFirstJourney',
      'KasTrustScopeJourney',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const AllowOnceJourney = {
  args: { scenario: 'allow' satisfies ApprovalScenario },
  component: ApprovalStory,
  parameters: certification('execute_bash requires approval', {
    pending: {
      label: 'one-time approval pending',
      coversVisualStates: ['allow-pending'],
      assertions: {
        visible: [
          'execute_bash requires approval',
          shellCommand,
          'Contract: pending | queue=1 | tool=pending',
        ],
      },
    },
    allowed: {
      label: 'one-time approval resolved',
      coversVisualStates: ['allowed'],
      assertions: {
        visible: [
          shellCommand,
          'Contract: selected allow_once | queue=0 | tool=approved',
        ],
        hidden: ['requires approval'],
      },
    },
  }),
  play: captureAllow,
};

export const RejectJourney = {
  args: { scenario: 'reject' satisfies ApprovalScenario },
  component: ApprovalStory,
  parameters: certification('execute_bash requires approval', {
    pending: {
      label: 'rejection decision pending',
      coversVisualStates: ['reject-pending'],
      assertions: {
        visible: [
          'execute_bash requires approval',
          'Contract: pending | queue=1 | tool=pending',
        ],
      },
    },
    rejected: {
      label: 'tool rejected through the store',
      coversVisualStates: ['rejected'],
      assertions: {
        visible: [
          'Rejected',
          'Contract: selected reject_once | queue=0 | tool=rejected',
        ],
        hidden: ['requires approval'],
      },
    },
  }),
  play: captureReject,
};

export const PermissionFirstJourney = {
  args: { scenario: 'permission-first' satisfies ApprovalScenario },
  component: ApprovalStory,
  parameters: certification('fs_write requires approval', {
    'permission-first': {
      label: 'permission payload rendered before ToolCall delivery',
      coversVisualStates: ['permission-first'],
      assertions: {
        visible: [
          'fs_write requires approval',
          'Write',
          '/workspace/report.ts',
          'export const ready = false;',
          'export const ready = true;',
          'Contract: pending | queue=1 | tool=absent',
        ],
        hidden: ['oldStr='],
      },
    },
    'permission-first-allowed': {
      label: 'permission-first request allowed without a synthetic history row',
      coversVisualStates: ['permission-first-allowed'],
      assertions: {
        visible: ['Contract: selected allow_once | queue=0 | tool=absent'],
        hidden: ['requires approval', '/workspace/report.ts'],
      },
    },
  }),
  play: capturePermissionFirst,
};

export const KasTrustScopeJourney = {
  args: { scenario: 'kas-trust-scope' satisfies ApprovalScenario },
  component: ApprovalStory,
  parameters: certification('execute_bash requires approval', {
    pending: {
      label: 'KAS approval before trust drill-in',
      assertions: {
        visible: [
          'execute_bash requires approval',
          'Always allow',
          'Contract: pending | queue=1 | tool=pending',
        ],
      },
    },
    'trust-session': {
      label: 'KAS exact and pattern trust choices at session scope',
      coversVisualStates: ['kas-trust-session'],
      assertions: {
        visible: [
          'trust [session]',
          'Trust "git status"',
          'Trust "git *"',
          'exact match',
          'session',
        ],
      },
    },
    'trust-workspace': {
      label: 'KAS trust scope cycled to workspace',
      coversVisualStates: ['kas-trust-workspace'],
      assertions: {
        visible: ['trust [workspace]', 'exact match', 'workspace'],
        hidden: ['trust [session]'],
      },
    },
    trusted: {
      label: 'KAS workspace trust resolved with exact gated resource',
      coversVisualStates: ['kas-trusted'],
      assertions: {
        visible: [
          'Contract: selected always-accept',
          'scope=workspace',
          'resource=git status',
          'queue=0',
          'tool=approved',
        ],
        hidden: ['requires approval'],
      },
    },
  }),
  play: captureKasTrustScope,
};
