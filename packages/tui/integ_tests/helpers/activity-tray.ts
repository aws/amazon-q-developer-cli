import { TestCase } from '../../src/test-utils/TestCase.js';
import { Feature } from '../../src/features.js';
import { AgentEventType } from '../../src/types/agent-events.js';

export const BACKSPACE = '\x7f';
export const CTRL_S = '\x13';
export const CTRL_X = '\x18';
export const LEGACY_SHIFT_DOWN = '\x1b[b';
export const SHIFT_DOWN = '\x1b[1;2B';
export const SHIFT_LEFT = '\x1b[1;2D';
export const SHIFT_RIGHT = '\x1b[1;2C';
export const TAB = '\t';

export async function launchActivityTrayCase(
  testName: string,
  options: { lite?: boolean } = {}
): Promise<TestCase> {
  let builder = TestCase.builder()
    .withTestName(testName)
    .withTimeout(15_000)
    .withEnv({
      KIRO_ENABLED_FEATURES: JSON.stringify([Feature.Workflows]),
      KIRO_TEST_MOCK_TURN_TIMEOUT_MS: '0',
    });
  if (options.lite) builder = builder.withLite();
  const testCase = await builder.launch();
  await testCase.waitForVisibleText('ask a question', 15_000);
  return testCase;
}

export async function eraseDraft(
  testCase: TestCase,
  characterCount: number
): Promise<void> {
  let currentValue = (await testCase.getStore()).commandInputValue;
  for (let index = 0; index < characterCount; index += 1) {
    await testCase.sendKeys(BACKSPACE);
    const state = await testCase.waitForStore(
      (value) => value.commandInputValue !== currentValue
    );
    currentValue = state.commandInputValue;
  }
  if (currentValue !== '') {
    throw new Error(`Expected an empty draft, received ${currentValue}`);
  }
}

export async function startWorkflow(
  testCase: TestCase,
  workflowId: string,
  workflowName: string,
  nodeNames: readonly string[]
): Promise<void> {
  await testCase.mockSessionUpdate({
    type: AgentEventType.WorkflowProgress,
    id: `${workflowId}-start`,
    event: {
      type: 'run_start',
      workflowId,
      parentSessionId: 'mock-session-id',
      workflowName,
      inputs: {},
      nodeTree: nodeNames.map((agentName, index) => ({
        nodeId: `${workflowId}-step-${index + 1}`,
        type: 'step',
        agentName,
      })),
    },
  });
}

export async function completeWorkflow(
  testCase: TestCase,
  workflowId: string,
  workflowName: string,
  nodeName: string
): Promise<void> {
  await testCase.mockSessionUpdate({
    type: AgentEventType.WorkflowProgress,
    id: `${workflowId}-complete`,
    event: {
      type: 'run_complete',
      workflowId,
      parentSessionId: 'mock-session-id',
      status: 'completed',
      finalState: {
        workflowId,
        workflowName,
        status: 'completed',
        inputs: {},
        artifacts: {},
        capturedOutputs: {},
        parentSessionId: 'mock-session-id',
        root: {
          nodeId: `${workflowId}-step-1`,
          type: 'step',
          status: 'completed',
          agentName: nodeName,
        },
      },
    },
  });
}

export interface TaskFixture {
  id: string;
  task_description: string;
  completed: boolean;
}

export async function updateTasks(
  testCase: TestCase,
  toolCallId: string,
  command: 'create' | 'complete',
  tasks: readonly TaskFixture[]
): Promise<void> {
  await testCase.mockSessionUpdate({
    type: AgentEventType.ToolCall,
    id: toolCallId,
    name: 'task',
    kind: 'other',
    args: { command },
  });
  await testCase.mockSessionUpdate({
    type: AgentEventType.ToolCallFinished,
    id: toolCallId,
    result: {
      status: 'success',
      output: JSON.stringify({ tasks }),
    },
  });
}
