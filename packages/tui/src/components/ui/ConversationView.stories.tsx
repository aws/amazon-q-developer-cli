import React, { useState } from 'react';
import { Box, useInput } from '../../renderer.js';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
  type StreamEventHandler,
  type ToolResult,
} from '../../stores/app-store.js';
import {
  AgentEventType,
  ContentType,
  type ToolCallDiffContent,
} from '../../types/agent-events.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../storybook/contracts.js';
import { VerbosityOverrideContext } from '../../hooks/useVerbose.js';
import { DEFAULT_DISPLAY } from '../../lite/verbose.js';
import { Settings } from '../../constants/settings.js';
import { ConversationView } from './ConversationView.js';

const viewport = { columns: 140, rows: 48 };

const meta = {
  title: 'UI/ConversationView',
  component: ConversationView,
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    visualStates: {
      'completed-history': {
        label: 'Multiple completed transcript turns',
      },
      'static-thinking': {
        label: 'Completed reasoning retained in static history',
      },
      'waiting-for-output': {
        label: 'Active turn waiting for the first model event',
      },
      'live-thinking': {
        label: 'Reasoning streamed into the active turn',
      },
      streaming: {
        label: 'Assistant response streaming in the active tail',
      },
      'static-promotion': {
        label: 'Completed active turn promoted to static history',
      },
      'sequential-tools': {
        label: 'Sequential read, write, and shell calls in one turn',
      },
      'parallel-tools': {
        label: 'Parallel read, write, and shell calls in one turn',
      },
      'tail-pressure': {
        label: 'Mixed transcript exceeding the dynamic tail limit',
      },
      'static-shell-output': {
        label: 'Completed shell result retained in static history',
      },
      'active-read': {
        label: 'Read tool active inside a mixed transcript',
      },
      'completed-read': {
        label: 'Read tool result inside a mixed transcript',
      },
      'active-write': {
        label: 'Write diff active inside a mixed transcript',
      },
      'completed-write': {
        label: 'Write diff completed inside a mixed transcript',
      },
      'active-shell': {
        label: 'Shell tool active inside a mixed transcript',
      },
      'streaming-shell-output': {
        label: 'Shell tool receiving incremental output',
      },
      'failed-shell': {
        label: 'Shell failure retained before the next tool',
      },
      'active-generic-tool': {
        label: 'Generic MCP tool active inside a mixed transcript',
      },
      'completed-generic-tool': {
        label: 'Generic MCP result inside a mixed transcript',
      },
      'mixed-tool-static-promotion': {
        label: 'Mixed tool lifecycle promoted before the next prompt',
      },
      'discovery-tool-composition': {
        label:
          'Grep, glob, directory, code, and documentation tools in one turn',
      },
      'coordination-tool-composition': {
        label: 'Web, image, subagent, and workflow tools in one completed turn',
      },
    },
    storyOrder: [
      'CompletedMultiTurnHistory',
      'StreamingToStaticPromotion',
      'ToolCombinationTailPressure',
      'ToolLifecycleStress',
      'DiscoveryToolComposition',
      'CoordinationToolComposition',
    ],
  },
};

export default meta;

function certification(
  readyText: string,
  assertions: NonNullable<
    NonNullable<StorybookParameters['certification']>['assertions']
  >,
  coversVisualStates: readonly string[],
  captures?: NonNullable<StorybookParameters['certification']>['captures']
): StorybookParameters {
  return {
    layout: 'fullscreen',
    capturesKeyboard: true,
    coversVisualStates,
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport,
      environment: { KIRO_LITE_ROLLOUT_ENABLED: '1' },
      assertions: {
        visible: assertions.visible,
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
      },
      ...(captures ? { captures } : {}),
    },
  };
}

function createStoryStore(): AppStoreApi {
  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'v2',
    uiMode: 'tui',
  });
  store.setState({
    currentAgent: { name: 'kiro' },
    settings: {
      [Settings.CHAT_GREETING_ENABLED]: false,
      [Settings.CHAT_SHOW_THINKING_TIPS]: false,
    },
  });
  return store;
}

function StorySurface({ store }: { store: AppStoreApi }): React.ReactElement {
  return (
    <AppStoreContext.Provider value={store}>
      <VerbosityOverrideContext.Provider
        value={{
          display: {
            ...DEFAULT_DISPLAY,
            showElapsed: false,
            thinkingDisplay: 'expanded',
            showThinkingContent: true,
            persistOutput: true,
          },
          filters: ['all'],
        }}
      >
        <Box flexDirection="column">
          <ConversationView />
        </Box>
      </VerbosityOverrideContext.Provider>
    </AppStoreContext.Provider>
  );
}

function userMessage(emit: StreamEventHandler, id: string, text: string): void {
  emit({
    type: AgentEventType.UserMessage,
    id,
    content: { type: ContentType.Text, text },
  });
}

function assistantContent(
  emit: StreamEventHandler,
  id: string,
  text: string
): void {
  emit({
    type: AgentEventType.Content,
    id,
    content: { type: ContentType.Text, text },
  });
}

function assistantThought(
  emit: StreamEventHandler,
  id: string,
  text: string
): void {
  emit({
    type: AgentEventType.Thought,
    id,
    content: { type: ContentType.Text, text },
  });
}

function startTurn(emit: StreamEventHandler): void {
  emit({ type: AgentEventType.TurnStart });
}

function endTurn(emit: StreamEventHandler): void {
  emit({ type: AgentEventType.TurnEnd });
  emit.flush();
}

function startRead(emit: StreamEventHandler, id: string, path: string): void {
  emit({
    type: AgentEventType.ToolCall,
    id,
    name: 'fs_read',
    kind: 'read',
    origin: 'builtin',
    args: {
      operations: [{ mode: 'Line', path, offset: 0, limit: 20 }],
    },
    locations: [{ path, line: 1 }],
  });
}

function finishRead(
  emit: StreamEventHandler,
  id: string,
  marker: string
): void {
  emit({
    type: AgentEventType.ToolCallFinished,
    id,
    result: {
      status: 'success',
      output: {
        items: [{ Text: `export const ${marker} = true;` }],
      },
    },
  });
}

function writeDiff(
  path: string,
  oldText: string,
  newText: string
): ToolCallDiffContent {
  return { type: 'diff', path, oldText, newText };
}

function startWrite(
  emit: StreamEventHandler,
  id: string,
  path: string,
  oldText: string,
  newText: string
): void {
  emit({
    type: AgentEventType.ToolCall,
    id,
    name: 'fs_write',
    kind: 'edit',
    origin: 'builtin',
    args: {
      command: 'strReplace',
      path,
      oldStr: oldText,
      newStr: newText,
    },
    toolContent: [writeDiff(path, oldText, newText)],
    locations: [{ path, line: 1 }],
  });
}

function finishWrite(
  emit: StreamEventHandler,
  id: string,
  path: string,
  oldText: string,
  newText: string
): void {
  emit({
    type: AgentEventType.ToolCallFinished,
    id,
    result: { status: 'success', output: '' },
    toolContent: [writeDiff(path, oldText, newText)],
  });
}

function startShell(
  emit: StreamEventHandler,
  id: string,
  command: string
): void {
  emit({
    type: AgentEventType.ToolCall,
    id,
    name: 'execute_bash',
    kind: 'execute',
    origin: 'builtin',
    args: { command, working_dir: '/workspace' },
  });
}

function finishShell(
  emit: StreamEventHandler,
  id: string,
  output: string
): void {
  emit({
    type: AgentEventType.ToolCallFinished,
    id,
    result: { status: 'success', output },
  });
}

function updateTool(emit: StreamEventHandler, id: string, text: string): void {
  emit({
    type: AgentEventType.ToolCallUpdate,
    id,
    content: { type: ContentType.Text, text },
  });
}

function failTool(
  emit: StreamEventHandler,
  id: string,
  error: string,
  output?: string
): void {
  emit({
    type: AgentEventType.ToolCallFinished,
    id,
    result: { status: 'error', error, output },
  });
}

function startGenericTool(emit: StreamEventHandler, id: string): void {
  emit({
    type: AgentEventType.ToolCall,
    id,
    name: 'search_reports',
    originalTitle: '@artifact/search_reports',
    kind: 'search',
    origin: 'mcp',
    args: { query: 'release certification', limit: 2 },
  });
}

function finishGenericTool(emit: StreamEventHandler, id: string): void {
  emit({
    type: AgentEventType.ToolCallFinished,
    id,
    result: {
      status: 'success',
      output: 'MCP_REPORT_RESULT two certification reports found',
    },
  });
}

function startToolCall(
  emit: StreamEventHandler,
  id: string,
  name: string,
  kind: string,
  args: Record<string, unknown>
): void {
  emit({
    type: AgentEventType.ToolCall,
    id,
    name,
    kind,
    origin: 'builtin',
    args,
  });
}

function finishToolCall(
  emit: StreamEventHandler,
  id: string,
  result: ToolResult
): void {
  emit({
    type: AgentEventType.ToolCallFinished,
    id,
    result,
  });
}

function createCompletedHistoryStore(): AppStoreApi {
  const store = createStoryStore();
  const emit = store.getState().createStreamEventHandler({ fromHistory: true });

  userMessage(
    emit,
    'history-user-1',
    'HISTORY_PROMPT_ONE audit the parser migration.'
  );
  startTurn(emit);
  assistantThought(
    emit,
    'history-thought-1',
    'HISTORY_REASONING compare the parser contracts before editing.'
  );
  assistantContent(
    emit,
    'history-model-1',
    'HISTORY_PLAN I will inspect, patch, and verify the parser.'
  );
  startRead(emit, 'history-read', '/workspace/src/history-parser.ts');
  finishRead(emit, 'history-read', 'HISTORY_READ_RESULT');
  startWrite(
    emit,
    'history-write',
    '/workspace/src/history-parser.ts',
    'export const parserReady = false;',
    'export const parserReady = true;'
  );
  finishWrite(
    emit,
    'history-write',
    '/workspace/src/history-parser.ts',
    'export const parserReady = false;',
    'export const parserReady = true;'
  );
  startShell(emit, 'history-shell', 'printf history-shell-check');
  finishShell(emit, 'history-shell', 'HISTORY_SHELL_RESULT passed');
  assistantContent(
    emit,
    'history-model-2',
    'HISTORY_SUMMARY parser migration verified.'
  );
  endTurn(emit);

  userMessage(
    emit,
    'history-user-2',
    'HISTORY_PROMPT_TWO summarize the evidence.'
  );
  startTurn(emit);
  assistantContent(
    emit,
    'history-model-3',
    'HISTORY_SECOND_RESPONSE read, write, and shell checks passed.'
  );
  endTurn(emit);

  userMessage(
    emit,
    'history-user-3',
    'HISTORY_PROMPT_THREE prepare the release note.'
  );

  return store;
}

function CompletedHistoryStory(): React.ReactElement {
  const [store] = useState(createCompletedHistoryStore);
  return <StorySurface store={store} />;
}

export const CompletedMultiTurnHistory = {
  render: CompletedHistoryStory,
  parameters: certification(
    'HISTORY_PROMPT_THREE',
    {
      visible: [
        'HISTORY_PROMPT_ONE',
        'HISTORY_REASONING',
        'HISTORY_PLAN',
        'Read /workspace/src/history-parser.ts',
        'Write /workspace/src/history-parser.ts',
        'Shell printf history-shell-check',
        'HISTORY_SHELL_RESULT',
        'HISTORY_SUMMARY',
        'HISTORY_PROMPT_TWO',
        'HISTORY_SECOND_RESPONSE',
        'HISTORY_PROMPT_THREE',
      ],
      ordered: [
        'HISTORY_PROMPT_ONE',
        'HISTORY_REASONING',
        'HISTORY_PLAN',
        'Read /workspace/src/history-parser.ts',
        'Write /workspace/src/history-parser.ts',
        'Shell printf history-shell-check',
        'HISTORY_SHELL_RESULT',
        'HISTORY_SUMMARY',
        'HISTORY_PROMPT_TWO',
        'HISTORY_SECOND_RESPONSE',
        'HISTORY_PROMPT_THREE',
      ],
      occurrences: {
        HISTORY_PROMPT_ONE: 1,
        HISTORY_SHELL_RESULT: 1,
        HISTORY_SUMMARY: 1,
        HISTORY_PROMPT_TWO: 1,
        HISTORY_SECOND_RESPONSE: 1,
        HISTORY_PROMPT_THREE: 1,
      },
    },
    [
      'completed-history',
      'static-thinking',
      'sequential-tools',
      'static-shell-output',
    ]
  ),
};

function StreamingPromotionStory(): React.ReactElement {
  const [{ store, emit }] = useState(() => {
    const storyStore = createStoryStore();
    const handler = storyStore.getState().createStreamEventHandler();
    userMessage(
      handler,
      'stream-user-1',
      'STREAM_PROMPT inspect the active response.'
    );
    startTurn(handler);
    return { store: storyStore, emit: handler };
  });
  const [step, setStep] = useState(0);

  useInput((_, key) => {
    if (!key.tab || step >= 3) return;
    const nextStep = step + 1;
    if (nextStep === 1) {
      assistantThought(
        emit,
        'stream-thought',
        'STREAM_REASONING validate the live slot before replying.'
      );
    } else if (nextStep === 2) {
      assistantContent(
        emit,
        'stream-response',
        'STREAM_RESPONSE the live slot is rendering this response.'
      );
    } else {
      endTurn(emit);
      userMessage(
        emit,
        'stream-user-2',
        'STREAM_NEXT_PROMPT confirms the prior turn is static.'
      );
    }
    setStep(nextStep);
  });

  return <StorySurface store={store} />;
}

const captureStreamingPromotion: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('waiting');
  await press('tab');
  await waitFor('STREAM_REASONING');
  await capture('reasoning');
  await press('tab');
  await waitFor('STREAM_RESPONSE');
  await capture('streaming');
  await press('tab');
  await waitFor('STREAM_NEXT_PROMPT');
  await capture('promoted-static');
};

export const StreamingToStaticPromotion = {
  render: StreamingPromotionStory,
  parameters: certification(
    'STREAM_PROMPT',
    {
      visible: ['STREAM_PROMPT'],
    },
    [],
    {
      waiting: {
        label: 'waiting for assistant output',
        coversVisualStates: ['waiting-for-output'],
        assertions: {
          visible: ['STREAM_PROMPT', 'Thinking'],
          hidden: ['STREAM_REASONING', 'STREAM_RESPONSE'],
        },
      },
      reasoning: {
        label: 'reasoning streamed into the active tail',
        coversVisualStates: ['live-thinking'],
        assertions: {
          visible: ['STREAM_PROMPT', 'STREAM_REASONING'],
          hidden: ['STREAM_RESPONSE', 'STREAM_NEXT_PROMPT'],
          ordered: ['STREAM_PROMPT', 'STREAM_REASONING'],
        },
      },
      streaming: {
        label: 'assistant text streamed after reasoning',
        coversVisualStates: ['streaming'],
        assertions: {
          visible: ['STREAM_REASONING', 'STREAM_RESPONSE'],
          hidden: ['STREAM_NEXT_PROMPT'],
          ordered: ['STREAM_REASONING', 'STREAM_RESPONSE'],
          occurrences: {
            STREAM_REASONING: 1,
            STREAM_RESPONSE: 1,
          },
        },
      },
      'promoted-static': {
        label: 'completed response promoted before the next prompt',
        coversVisualStates: ['static-promotion', 'static-thinking'],
        assertions: {
          visible: [
            'STREAM_PROMPT',
            'STREAM_REASONING',
            'STREAM_RESPONSE',
            'STREAM_NEXT_PROMPT',
          ],
          ordered: [
            'STREAM_PROMPT',
            'STREAM_REASONING',
            'STREAM_RESPONSE',
            'STREAM_NEXT_PROMPT',
          ],
          occurrences: {
            STREAM_PROMPT: 1,
            STREAM_REASONING: 1,
            STREAM_RESPONSE: 1,
            STREAM_NEXT_PROMPT: 1,
          },
        },
      },
    }
  ),
  play: captureStreamingPromotion,
};

function ToolPressureStory(): React.ReactElement {
  const [{ store, emit }] = useState(() => {
    const storyStore = createStoryStore();
    const handler = storyStore.getState().createStreamEventHandler();
    userMessage(
      handler,
      'tools-user-1',
      'TOOLS_PROMPT inspect and update both parser paths.'
    );
    startTurn(handler);
    assistantContent(
      handler,
      'tools-intro',
      'TOOLS_PLAN run sequential checks before the parallel batch.'
    );
    return { store: storyStore, emit: handler };
  });
  const [step, setStep] = useState(0);

  useInput((_, key) => {
    if (!key.tab || step >= 6) return;
    const nextStep = step + 1;
    if (nextStep === 1) {
      startRead(emit, 'seq-read', '/workspace/src/sequential-read.ts');
      finishRead(emit, 'seq-read', 'SEQ_READ_RESULT');
    } else if (nextStep === 2) {
      startWrite(
        emit,
        'seq-write',
        '/workspace/src/sequential-write.ts',
        'export const sequential = false;',
        'export const sequential = true;'
      );
      finishWrite(
        emit,
        'seq-write',
        '/workspace/src/sequential-write.ts',
        'export const sequential = false;',
        'export const sequential = true;'
      );
    } else if (nextStep === 3) {
      startShell(emit, 'seq-shell', 'printf sequential-shell-check');
      finishShell(emit, 'seq-shell', 'SEQ_SHELL_RESULT passed');
    } else if (nextStep === 4) {
      startRead(emit, 'parallel-read', '/workspace/src/parallel-read.ts');
      startWrite(
        emit,
        'parallel-write',
        '/workspace/src/parallel-write.ts',
        'export const parallel = false;',
        'export const parallel = true;'
      );
      startShell(emit, 'parallel-shell', 'printf parallel-shell-check');
    } else if (nextStep === 5) {
      finishShell(emit, 'parallel-shell', 'PARALLEL_SHELL_RESULT passed');
      finishWrite(
        emit,
        'parallel-write',
        '/workspace/src/parallel-write.ts',
        'export const parallel = false;',
        'export const parallel = true;'
      );
      finishRead(emit, 'parallel-read', 'PARALLEL_READ_RESULT');
      assistantContent(
        emit,
        'tools-summary',
        'TOOLS_STRESS_SUMMARY all six tool calls completed.'
      );
    } else {
      endTurn(emit);
      userMessage(
        emit,
        'tools-user-2',
        'TOOLS_NEXT_PROMPT verify the pressure turn stayed ordered.'
      );
    }
    setStep(nextStep);
  });

  return <StorySurface store={store} />;
}

const captureToolPressure: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await waitFor('TOOLS_PLAN');
  await capture('plan');
  await press('tab');
  await waitFor('/workspace/src/sequential-read.ts');
  await capture('sequential-read');
  await press('tab');
  await waitFor('/workspace/src/sequential-write.ts');
  await capture('sequential-write');
  await press('tab');
  await waitFor('sequential-shell-check');
  await capture('sequential-shell');
  await press('tab');
  await waitFor('parallel-shell-check');
  await capture('parallel-running');
  await press('tab');
  await waitFor('TOOLS_STRESS_SUMMARY');
  await capture('parallel-completed');
  await press('tab');
  await waitFor('TOOLS_NEXT_PROMPT');
  await capture('pressure-promoted');
};

export const ToolCombinationTailPressure = {
  render: ToolPressureStory,
  parameters: certification(
    'TOOLS_PROMPT',
    {
      visible: ['TOOLS_PROMPT'],
    },
    [],
    {
      plan: {
        label: 'tool plan streaming before execution',
        assertions: {
          visible: ['TOOLS_PROMPT', 'TOOLS_PLAN'],
          hidden: ['/workspace/src/sequential-read.ts'],
          ordered: ['TOOLS_PROMPT', 'TOOLS_PLAN'],
        },
      },
      'sequential-read': {
        label: 'sequential read completed',
        coversVisualStates: ['sequential-tools'],
        assertions: {
          visible: [
            'TOOLS_PLAN',
            'Read /workspace/src/sequential-read.ts',
            'SEQ_READ_RESULT',
          ],
          hidden: ['/workspace/src/sequential-write.ts'],
          ordered: [
            'TOOLS_PLAN',
            'Read /workspace/src/sequential-read.ts',
            'SEQ_READ_RESULT',
          ],
        },
      },
      'sequential-write': {
        label: 'sequential write completed after read',
        coversVisualStates: ['sequential-tools'],
        assertions: {
          visible: [
            'Read /workspace/src/sequential-read.ts',
            'Write /workspace/src/sequential-write.ts',
            'export const sequential = false;',
            'export const sequential = true;',
          ],
          hidden: ['sequential-shell-check'],
          ordered: [
            'Read /workspace/src/sequential-read.ts',
            'Write /workspace/src/sequential-write.ts',
          ],
        },
      },
      'sequential-shell': {
        label: 'sequential shell completed after read and write',
        coversVisualStates: ['sequential-tools'],
        assertions: {
          visible: [
            'Read /workspace/src/sequential-read.ts',
            'Write /workspace/src/sequential-write.ts',
            'Shell printf sequential-shell-check',
            'SEQ_SHELL_RESULT',
          ],
          hidden: ['/workspace/src/parallel-read.ts'],
          ordered: [
            'Read /workspace/src/sequential-read.ts',
            'Write /workspace/src/sequential-write.ts',
            'Shell printf sequential-shell-check',
          ],
        },
      },
      'parallel-running': {
        label: 'parallel read, write, and shell calls active together',
        coversVisualStates: ['parallel-tools', 'tail-pressure'],
        assertions: {
          visible: [
            'Read /workspace/src/sequential-read.ts',
            'Write /workspace/src/sequential-write.ts',
            'Shell printf sequential-shell-check',
            'Read /workspace/src/parallel-read.ts',
            'Write /workspace/src/parallel-write.ts',
            'Shell printf parallel-shell-check',
          ],
          hidden: ['TOOLS_STRESS_SUMMARY'],
          ordered: [
            'Read /workspace/src/sequential-read.ts',
            'Write /workspace/src/sequential-write.ts',
            'Shell printf sequential-shell-check',
            'Read /workspace/src/parallel-read.ts',
            'Write /workspace/src/parallel-write.ts',
            'Shell printf parallel-shell-check',
          ],
          occurrences: {
            '/workspace/src/sequential-read.ts': 1,
            '/workspace/src/sequential-write.ts': 1,
            'sequential-shell-check': 1,
            '/workspace/src/parallel-read.ts': 1,
            '/workspace/src/parallel-write.ts': 1,
            'parallel-shell-check': 1,
          },
        },
      },
      'parallel-completed': {
        label: 'parallel batch completed in reverse order',
        coversVisualStates: ['parallel-tools', 'tail-pressure', 'streaming'],
        assertions: {
          visible: [
            'Read /workspace/src/parallel-read.ts',
            'Write /workspace/src/parallel-write.ts',
            'Shell printf parallel-shell-check',
            'PARALLEL_READ_RESULT',
            'PARALLEL_SHELL_RESULT',
            'TOOLS_STRESS_SUMMARY',
          ],
          ordered: [
            'Read /workspace/src/parallel-read.ts',
            'Write /workspace/src/parallel-write.ts',
            'Shell printf parallel-shell-check',
            'TOOLS_STRESS_SUMMARY',
          ],
          occurrences: {
            PARALLEL_READ_RESULT: 1,
            PARALLEL_SHELL_RESULT: 1,
            TOOLS_STRESS_SUMMARY: 1,
          },
        },
      },
      'pressure-promoted': {
        label: 'pressure turn promoted without loss or duplication',
        coversVisualStates: [
          'static-promotion',
          'sequential-tools',
          'parallel-tools',
          'tail-pressure',
          'static-shell-output',
        ],
        assertions: {
          visible: [
            'TOOLS_PROMPT',
            'Read /workspace/src/sequential-read.ts',
            'Write /workspace/src/sequential-write.ts',
            'Shell printf sequential-shell-check',
            'SEQ_SHELL_RESULT',
            'Read /workspace/src/parallel-read.ts',
            'Write /workspace/src/parallel-write.ts',
            'Shell printf parallel-shell-check',
            'PARALLEL_SHELL_RESULT',
            'TOOLS_STRESS_SUMMARY',
            'TOOLS_NEXT_PROMPT',
          ],
          ordered: [
            'TOOLS_PROMPT',
            'Read /workspace/src/sequential-read.ts',
            'Write /workspace/src/sequential-write.ts',
            'Shell printf sequential-shell-check',
            'SEQ_SHELL_RESULT',
            'Read /workspace/src/parallel-read.ts',
            'Write /workspace/src/parallel-write.ts',
            'Shell printf parallel-shell-check',
            'PARALLEL_SHELL_RESULT',
            'TOOLS_STRESS_SUMMARY',
            'TOOLS_NEXT_PROMPT',
          ],
          occurrences: {
            TOOLS_PROMPT: 1,
            '/workspace/src/sequential-read.ts': 1,
            '/workspace/src/sequential-write.ts': 1,
            'sequential-shell-check': 1,
            SEQ_SHELL_RESULT: 1,
            '/workspace/src/parallel-read.ts': 1,
            '/workspace/src/parallel-write.ts': 1,
            'parallel-shell-check': 1,
            PARALLEL_SHELL_RESULT: 1,
            TOOLS_STRESS_SUMMARY: 1,
            TOOLS_NEXT_PROMPT: 1,
          },
        },
      },
    }
  ),
  play: captureToolPressure,
};

function ToolLifecycleStory(): React.ReactElement {
  const [{ store, emit }] = useState(() => {
    const storyStore = createStoryStore();
    const handler = storyStore.getState().createStreamEventHandler();
    userMessage(
      handler,
      'lifecycle-user-1',
      'LIFECYCLE_PROMPT exercise every tool transition.'
    );
    startTurn(handler);
    assistantContent(
      handler,
      'lifecycle-plan',
      'LIFECYCLE_PLAN read, edit, execute, and query the report service.'
    );
    startRead(handler, 'lifecycle-read', '/workspace/src/lifecycle-reader.ts');
    return { store: storyStore, emit: handler };
  });
  const [step, setStep] = useState(0);

  useInput((_, key) => {
    if (!key.tab || step >= 9) return;
    const next = step + 1;
    if (next === 1) {
      finishRead(emit, 'lifecycle-read', 'LIFECYCLE_READ_RESULT');
    } else if (next === 2) {
      startWrite(
        emit,
        'lifecycle-write',
        '/workspace/src/lifecycle-writer.ts',
        'export const lifecycleReady = false;',
        'export const lifecycleReady = true;'
      );
    } else if (next === 3) {
      finishWrite(
        emit,
        'lifecycle-write',
        '/workspace/src/lifecycle-writer.ts',
        'export const lifecycleReady = false;',
        'export const lifecycleReady = true;'
      );
    } else if (next === 4) {
      startShell(emit, 'lifecycle-shell', 'bun test lifecycle');
    } else if (next === 5) {
      updateTool(
        emit,
        'lifecycle-shell',
        'LIFECYCLE_SHELL_STREAM compiling\\n'
      );
    } else if (next === 6) {
      failTool(
        emit,
        'lifecycle-shell',
        'LIFECYCLE_SHELL_FAILURE exit code 1',
        'LIFECYCLE_SHELL_OUTPUT one assertion failed'
      );
    } else if (next === 7) {
      startGenericTool(emit, 'lifecycle-mcp');
    } else if (next === 8) {
      finishGenericTool(emit, 'lifecycle-mcp');
      assistantContent(
        emit,
        'lifecycle-summary',
        'LIFECYCLE_SUMMARY mixed tool outcomes recorded.'
      );
    } else {
      endTurn(emit);
      userMessage(
        emit,
        'lifecycle-user-2',
        'LIFECYCLE_NEXT_PROMPT confirms static promotion.'
      );
    }
    setStep(next);
  });

  return <StorySurface store={store} />;
}

const captureToolLifecycle: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await waitFor('/workspace/src/lifecycle-reader.ts');
  await capture('read-active');
  await press('tab');
  await waitFor('LIFECYCLE_READ_RESULT');
  await capture('read-completed');
  await press('tab');
  await waitFor('/workspace/src/lifecycle-writer.ts');
  await capture('write-active');
  await press('tab');
  await waitFor('export const lifecycleReady = true;');
  await capture('write-completed');
  await press('tab');
  await waitFor('bun test lifecycle');
  await capture('shell-active');
  await press('tab');
  await waitFor('LIFECYCLE_SHELL_STREAM');
  await capture('shell-streaming');
  await press('tab');
  await waitFor('LIFECYCLE_SHELL_FAILURE');
  await capture('shell-failed');
  await press('tab');
  await waitFor('search_reports');
  await capture('mcp-active');
  await press('tab');
  await waitFor('LIFECYCLE_SUMMARY');
  await capture('mcp-completed');
  await press('tab');
  await waitFor('LIFECYCLE_NEXT_PROMPT');
  await capture('promoted-static');
};

export const ToolLifecycleStress = {
  render: ToolLifecycleStory,
  parameters: certification(
    'LIFECYCLE_PROMPT',
    { visible: ['LIFECYCLE_PROMPT'] },
    [],
    {
      'read-active': {
        label: 'read tool active',
        coversVisualStates: ['active-read'],
        assertions: {
          visible: [
            'LIFECYCLE_PLAN',
            'Read /workspace/src/lifecycle-reader.ts',
          ],
          hidden: ['LIFECYCLE_READ_RESULT', 'lifecycle-writer.ts'],
        },
      },
      'read-completed': {
        label: 'read tool completed',
        coversVisualStates: ['completed-read'],
        assertions: {
          visible: [
            'Read /workspace/src/lifecycle-reader.ts',
            'LIFECYCLE_READ_RESULT',
          ],
          ordered: [
            'Read /workspace/src/lifecycle-reader.ts',
            'LIFECYCLE_READ_RESULT',
          ],
        },
      },
      'write-active': {
        label: 'write diff active',
        coversVisualStates: ['active-write'],
        assertions: {
          visible: [
            'Write /workspace/src/lifecycle-writer.ts',
            'export const lifecycleReady = false;',
            'export const lifecycleReady = true;',
          ],
          hidden: ['bun test lifecycle'],
        },
      },
      'write-completed': {
        label: 'write diff completed',
        coversVisualStates: ['completed-write'],
        assertions: {
          visible: [
            'Write /workspace/src/lifecycle-writer.ts',
            'export const lifecycleReady = false;',
            'export const lifecycleReady = true;',
          ],
        },
      },
      'shell-active': {
        label: 'shell tool active',
        coversVisualStates: ['active-shell'],
        assertions: {
          visible: ['Shell bun test lifecycle'],
          hidden: ['LIFECYCLE_SHELL_STREAM', 'LIFECYCLE_SHELL_FAILURE'],
        },
      },
      'shell-streaming': {
        label: 'shell output streaming',
        coversVisualStates: ['streaming-shell-output'],
        assertions: {
          visible: ['Shell bun test lifecycle', 'LIFECYCLE_SHELL_STREAM'],
          hidden: ['LIFECYCLE_SHELL_FAILURE'],
          ordered: ['Shell bun test lifecycle', 'LIFECYCLE_SHELL_STREAM'],
        },
      },
      'shell-failed': {
        label: 'shell tool failed',
        coversVisualStates: ['failed-shell'],
        assertions: {
          visible: [
            'Shell bun test lifecycle',
            'LIFECYCLE_SHELL_FAILURE',
            'LIFECYCLE_SHELL_OUTPUT',
          ],
          hidden: ['search_reports'],
        },
      },
      'mcp-active': {
        label: 'generic MCP tool active',
        coversVisualStates: ['active-generic-tool'],
        assertions: {
          visible: ['search_reports', 'query=release certification', 'limit=2'],
          hidden: ['MCP_REPORT_RESULT', 'LIFECYCLE_SUMMARY'],
        },
      },
      'mcp-completed': {
        label: 'generic MCP tool completed',
        coversVisualStates: ['completed-generic-tool'],
        assertions: {
          visible: ['search_reports', 'MCP_REPORT_RESULT', 'LIFECYCLE_SUMMARY'],
          ordered: ['search_reports', 'MCP_REPORT_RESULT', 'LIFECYCLE_SUMMARY'],
        },
      },
      'promoted-static': {
        label: 'mixed tool lifecycle promoted to static history',
        coversVisualStates: ['mixed-tool-static-promotion'],
        assertions: {
          visible: [
            'LIFECYCLE_SHELL_FAILURE',
            'search_reports',
            'MCP_REPORT_RESULT',
            'LIFECYCLE_SUMMARY',
            'LIFECYCLE_NEXT_PROMPT',
          ],
          ordered: [
            'LIFECYCLE_SHELL_FAILURE',
            'search_reports',
            'MCP_REPORT_RESULT',
            'LIFECYCLE_SUMMARY',
            'LIFECYCLE_NEXT_PROMPT',
          ],
          occurrences: {
            search_reports: 1,
            MCP_REPORT_RESULT: 1,
            LIFECYCLE_SUMMARY: 1,
            LIFECYCLE_NEXT_PROMPT: 1,
          },
        },
      },
    }
  ),
  play: captureToolLifecycle,
};

function createDiscoveryToolStore(): AppStoreApi {
  const store = createStoryStore();
  const emit = store.getState().createStreamEventHandler({ fromHistory: true });

  userMessage(
    emit,
    'discovery-user',
    'DISCOVERY_PROMPT map the visual certification implementation.'
  );
  startTurn(emit);
  assistantContent(
    emit,
    'discovery-plan',
    'DISCOVERY_PLAN search the catalog, inspect its files, and read the contract.'
  );

  startToolCall(emit, 'discovery-grep', 'grep', 'search', {
    pattern: 'VISUAL_CONTRACT',
    path: '/workspace/packages/tui/src',
  });
  finishToolCall(emit, 'discovery-grep', {
    status: 'success',
    output: {
      items: [
        {
          Json: {
            numMatches: 2,
            numFiles: 1,
            truncated: false,
            results: [
              {
                file: 'src/storybook/contracts.ts',
                count: 2,
                matches: [
                  '18:export const VISUAL_CONTRACT = true;',
                  '42:const marker = VISUAL_CONTRACT;',
                ],
              },
            ],
          },
        },
      ],
    },
  });

  startToolCall(emit, 'discovery-glob', 'glob', 'search', {
    pattern: 'src/**/*.stories.tsx',
  });
  finishToolCall(emit, 'discovery-glob', {
    status: 'success',
    output: {
      filePaths: [
        'src/components/ui/ConversationView.stories.tsx',
        'src/components/chat/tools/Shell.stories.tsx',
      ],
      totalFiles: 2,
      truncated: false,
    },
  });

  startToolCall(emit, 'discovery-ls', 'ls', 'read', {
    path: '/workspace/packages/tui/src/storybook',
  });
  finishToolCall(emit, 'discovery-ls', {
    status: 'success',
    output: {
      items: [
        {
          Text: [
            'User id: 501',
            '-rw-r--r-- 1 501 20 1024 Jan 15 10:30 /workspace/packages/tui/src/storybook/contracts.ts',
            '-rw-r--r-- 1 501 20 1024 Jan 15 10:30 /workspace/packages/tui/src/storybook/visual-coverage.ts',
          ].join('\n'),
        },
      ],
    },
  });

  startToolCall(emit, 'discovery-code', 'code', 'search', {
    operation: 'search_symbols',
    symbol_name: 'ConversationView',
  });
  finishToolCall(emit, 'discovery-code', {
    status: 'success',
    output: {
      items: [
        {
          Text: '[Function ConversationView @ src/components/ui/ConversationView.tsx:45-180 | ConversationView, Type ConversationTurn @ src/utils/group-turns.ts:8-24 | ConversationTurn]',
        },
      ],
    },
  });

  startToolCall(emit, 'discovery-docs', 'introspect', 'read', {
    query: 'visual story capture contracts',
  });
  finishToolCall(emit, 'discovery-docs', {
    status: 'success',
    output: {
      items: [
        {
          Text: 'DISCOVERY_DOC_RESULT captures bind semantic assertions to named visual states.',
        },
      ],
    },
  });

  assistantContent(
    emit,
    'discovery-summary',
    'DISCOVERY_SUMMARY all discovery surfaces remained ordered.'
  );
  endTurn(emit);
  userMessage(
    emit,
    'discovery-next',
    'DISCOVERY_NEXT_PROMPT preserve this as static evidence.'
  );
  return store;
}

function DiscoveryToolStory(): React.ReactElement {
  const [store] = useState(createDiscoveryToolStore);
  return <StorySurface store={store} />;
}

export const DiscoveryToolComposition = {
  render: DiscoveryToolStory,
  parameters: certification(
    'DISCOVERY_NEXT_PROMPT',
    {
      visible: [
        'DISCOVERY_PROMPT',
        'DISCOVERY_PLAN',
        'Grep "VISUAL_CONTRACT"',
        'contracts.ts',
        'Glob "src/**/*.stories.tsx"',
        'ConversationView.stories.tsx',
        'Ls /workspace/packages/tui/src/storybook',
        'visual-coverage.ts',
        'Code ConversationView',
        'Searched symbols',
        'Introspect visual story capture contracts',
        'DISCOVERY_DOC_RESULT',
        'DISCOVERY_SUMMARY',
        'DISCOVERY_NEXT_PROMPT',
      ],
      ordered: [
        'DISCOVERY_PROMPT',
        'DISCOVERY_PLAN',
        'Grep "VISUAL_CONTRACT"',
        'Glob "src/**/*.stories.tsx"',
        'Ls /workspace/packages/tui/src/storybook',
        'Code ConversationView',
        'Introspect visual story capture contracts',
        'DISCOVERY_SUMMARY',
        'DISCOVERY_NEXT_PROMPT',
      ],
      occurrences: {
        'Grep "VISUAL_CONTRACT"': 1,
        'Glob "src/**/*.stories.tsx"': 1,
        'Ls /workspace/packages/tui/src/storybook': 1,
        'Code ConversationView': 1,
        'Introspect visual story capture contracts': 1,
        DISCOVERY_SUMMARY: 1,
        DISCOVERY_NEXT_PROMPT: 1,
      },
    },
    ['discovery-tool-composition']
  ),
};

function createCoordinationToolStore(): AppStoreApi {
  const store = createStoryStore();
  const emit = store.getState().createStreamEventHandler({ fromHistory: true });

  userMessage(
    emit,
    'coordination-user',
    'COORDINATION_PROMPT collect external evidence and delegate verification.'
  );
  startTurn(emit);
  assistantContent(
    emit,
    'coordination-plan',
    'COORDINATION_PLAN search, fetch, inspect the image, delegate, then launch certification.'
  );

  startToolCall(emit, 'coordination-search', 'web_search', 'search', {
    query: 'terminal accessibility visual testing',
  });
  finishToolCall(emit, 'coordination-search', {
    status: 'success',
    output: { items: [{ Text: 'SEARCH_EVIDENCE' }] },
  });

  startToolCall(emit, 'coordination-fetch', 'web_fetch', 'read', {
    url: 'https://example.com/terminal-testing',
    search_terms: 'accessibility',
  });
  finishToolCall(emit, 'coordination-fetch', {
    status: 'success',
    output: { items: [{ Text: 'FETCH_EVIDENCE' }] },
  });

  startToolCall(emit, 'coordination-image', 'image_read', 'read', {
    paths: ['artifacts/visual-report.png'],
  });
  finishToolCall(emit, 'coordination-image', {
    status: 'success',
    output: '',
  });

  startToolCall(emit, 'coordination-session', 'session_management', 'other', {
    command: 'spawn_session',
    agent_name: 'visual-auditor',
    task: 'Audit the composed transcript evidence',
  });
  finishToolCall(emit, 'coordination-session', {
    status: 'success',
    output: JSON.stringify({ sessionId: 'visual-auditor-session' }),
  });

  startToolCall(emit, 'coordination-workflow', 'run_workflow', 'other', {
    workflow: {
      name: 'visual-certification',
      steps: [
        { type: 'step', name: 'capture' },
        {
          type: 'parallel',
          branches: [
            { type: 'step', name: 'semantic-checks' },
            { type: 'step', name: 'coverage-audit' },
          ],
        },
      ],
    },
  });
  finishToolCall(emit, 'coordination-workflow', {
    status: 'success',
    output: JSON.stringify({ workflowId: 'visual-certification' }),
  });

  assistantContent(
    emit,
    'coordination-summary',
    'COORDINATION_SUMMARY external and delegated evidence completed.'
  );
  endTurn(emit);
  userMessage(
    emit,
    'coordination-next',
    'COORDINATION_NEXT_PROMPT retain the composed result.'
  );
  return store;
}

function CoordinationToolStory(): React.ReactElement {
  const [store] = useState(createCoordinationToolStore);
  return <StorySurface store={store} />;
}

export const CoordinationToolComposition = {
  render: CoordinationToolStory,
  parameters: certification(
    'COORDINATION_NEXT_PROMPT',
    {
      visible: [
        'COORDINATION_PROMPT',
        'COORDINATION_PLAN',
        'WebSearch "terminal accessibility visual testing"',
        'WebFetch example.com/terminal-testing',
        'ImageRead visual-report.png',
        'Audit the composed transcript evidence',
        'Started workflow',
        '"visual-certification"',
        'COORDINATION_SUMMARY',
        'COORDINATION_NEXT_PROMPT',
      ],
      ordered: [
        'COORDINATION_PROMPT',
        'COORDINATION_PLAN',
        'WebSearch "terminal accessibility visual testing"',
        'WebFetch example.com/terminal-testing',
        'ImageRead visual-report.png',
        'Audit the composed transcript evidence',
        'Started workflow',
        'COORDINATION_SUMMARY',
        'COORDINATION_NEXT_PROMPT',
      ],
      occurrences: {
        'WebSearch "terminal accessibility visual testing"': 1,
        'WebFetch example.com/terminal-testing': 1,
        'ImageRead visual-report.png': 1,
        'Audit the composed transcript evidence': 1,
        'Started workflow': 1,
        COORDINATION_SUMMARY: 1,
        COORDINATION_NEXT_PROMPT: 1,
      },
    },
    ['coordination-tool-composition']
  ),
};
