import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import type { WorkflowEvent } from '../../src/types/workflow';
import type { WorkflowRunSummary } from '../../src/types/workflow-history';
import {
  AcpTestCase,
  type AcpTestCaseOptions,
} from '../../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './default-agent';

export const WORKFLOW_PARENT_SESSION_ID = 'workflow-parent';

export function createWorkflowTestCase(
  testName: string,
  options: Omit<AcpTestCaseOptions, 'testName'> = {}
): AcpTestCase {
  const { extraEnv, settings, terminalSize, ...rest } = options;
  return new AcpTestCase({
    ...rest,
    testName,
    terminalSize: terminalSize ?? { width: 120, height: 34 },
    settings: { ...settings, 'chat.enableWorkflows': true },
    extraEnv: {
      ...extraEnv,
      KIRO_ENABLED_FEATURES: '["workflows"]',
    },
  });
}

export function setupWorkflowHandshake(
  tc: AcpTestCase,
  listRuns: () => WorkflowRunSummary[] = () => []
): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: WORKFLOW_PARENT_SESSION_ID,
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
  tc.mock.on('_kiro/workflow/list', () => ({ runs: listRuns() }));
}

export async function launchWorkflowCase(tc: AcpTestCase): Promise<void> {
  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForStore((state) => state.isInitialized, 10_000);
}

export function notifyWorkflowEvent(
  tc: AcpTestCase,
  event: WorkflowEvent
): void {
  if (event.type === 'node_start') {
    const { type: _eventType, nodeType, ...nodePayload } = event;
    tc.mock.notify('_kiro/workflow/node_start', {
      ...nodePayload,
      type: nodeType,
    });
    return;
  }
  const { type, ...payload } = event;
  tc.mock.notify(`_kiro/workflow/${type}`, payload);
}

export function workflowRunSummary(
  workflowId: string,
  name: string,
  status: WorkflowRunSummary['status']
): WorkflowRunSummary {
  return {
    workflowId,
    name,
    status,
    createdAt: '2026-08-15T10:00:00.000Z',
    updatedAt: '2026-08-15T10:01:00.000Z',
    parentSessionId: WORKFLOW_PARENT_SESSION_ID,
  };
}
