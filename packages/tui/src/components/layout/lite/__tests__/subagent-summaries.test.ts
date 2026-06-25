import { describe, expect, test } from 'vitest';
import stripAnsi from 'strip-ansi';
import { MessageRole, type MessageType } from '../../../../stores/app-store.js';
import { DEFAULT_DISPLAY } from '../../../../lite/verbose.js';
import type { AgentSession } from '../../../../types/multi-session.js';
import { renderSubagentFinalBlock } from '../../../../lite/render.js';
import {
  collectSubagentSummariesByParent,
  markSubagentSummariesEmitted,
  renderPendingSubagentSummaryAppendices,
  renderSubagentSummaryAppendix,
  selectUnemittedSubagentSummaries,
  selectPendingSubagentSummaryEntries,
  shouldRenderSubagentResponseSummaries,
  subagentSummaryKey,
} from '../subagent-summaries.js';

function parent(id: string, group: string): MessageType {
  return {
    id,
    role: MessageRole.ToolUse,
    name: 'orchestrate_subagent',
    content: '{}',
    pipelineGroupId: group,
    isFinished: true,
  };
}

function toolUse(
  id: string,
  name: string,
  content: Record<string, unknown>,
  agentName?: string
): MessageType {
  return {
    id,
    role: MessageRole.ToolUse,
    name,
    content: JSON.stringify(content),
    isFinished: true,
    ...(agentName ? { agentName } : {}),
  };
}

function summary(
  id: string,
  content: Record<string, unknown>,
  agentName?: string
): MessageType {
  return toolUse(id, 'summary', content, agentName);
}

function kasResponse(id: string, response: string, agentName: string) {
  return toolUse(id, 'Subagent Response', { response, files: [] }, agentName);
}

function session(id: string, name: string, group: string): AgentSession {
  return {
    id,
    name,
    agentName: name,
    group,
    role: name,
    status: 'terminated',
    type: 'ephemeral',
    created: new Date(0),
    lastActivity: new Date(0),
  };
}

describe('collectSubagentSummariesByParent', () => {
  test('collects KAS pipeline summaries from child session conversations', () => {
    const messages = [parent('parent-1', 'crew-1')];
    const sessions = new Map([
      ['sub-chat', session('sub-chat', 'explore-components', 'crew-1')],
      [
        'sub-layout',
        session('sub-layout', 'explore-utils-and-parsers', 'crew-1'),
      ],
    ]);
    const conversations = new Map<string, MessageType[]>([
      [
        'sub-chat',
        [
          summary('summary-chat', {
            contextSummary: 'components digest',
            taskResult: 'read src/components/chat',
          }),
        ],
      ],
      [
        'sub-layout',
        [
          summary('summary-layout', {
            contextSummary: '',
            taskResult: 'read src/components/layout',
          }),
        ],
      ],
    ]);

    const out = collectSubagentSummariesByParent(
      messages,
      sessions,
      conversations,
      'kiro'
    );

    expect(out.get('parent-1')).toEqual([
      {
        stageName: 'explore-components',
        contextSummary: 'components digest',
        taskResult: 'read src/components/chat',
      },
      {
        stageName: 'explore-utils-and-parsers',
        contextSummary: '',
        taskResult: 'read src/components/layout',
      },
    ]);
  });

  test('deduplicates summaries already present in the main message stream', () => {
    const messages = [
      parent('parent-1', 'crew-1'),
      summary(
        'summary-main',
        {
          contextSummary: 'components digest',
          taskResult: 'read src/components/chat',
        },
        'explore-components'
      ),
    ];
    const sessions = new Map([
      ['sub-chat', session('sub-chat', 'explore-components', 'crew-1')],
    ]);
    const conversations = new Map<string, MessageType[]>([
      [
        'sub-chat',
        [
          summary('summary-chat', {
            contextSummary: 'components digest',
            taskResult: 'read src/components/chat',
          }),
        ],
      ],
    ]);

    const out = collectSubagentSummariesByParent(
      messages,
      sessions,
      conversations,
      'kiro'
    );

    expect(out.get('parent-1')).toEqual([
      {
        stageName: 'explore-components',
        contextSummary: 'components digest',
        taskResult: 'read src/components/chat',
      },
    ]);
  });

  test('collects KAS subagent_response output and renders it ONCE (no full-output dup)', () => {
    const parentMsg = {
      ...parent('parent-1', 'crew-1'),
      result: { status: 'success', output: 'done' },
    } as MessageType;
    const kasOutput = Array.from(
      { length: 35 },
      (_, i) => `FULLOUTPUTKAS-${i}`
    ).join('\n');
    const messages = [parentMsg];
    const sessions = new Map([
      ['sub-respond', session('sub-respond', 'respond', 'crew-1')],
    ]);
    const conversations = new Map<string, MessageType[]>([
      ['sub-respond', [kasResponse('response-1', kasOutput, 'respond')]],
    ]);

    const summaries = collectSubagentSummariesByParent(
      messages,
      sessions,
      conversations,
      'kiro'
    );

    expect(summaries.get('parent-1')).toEqual([
      {
        stageName: 'respond',
        kind: 'response',
        contextSummary: '',
        taskResult: kasOutput,
      },
    ]);

    const parentToolMsg = parentMsg as Extract<
      MessageType,
      { role: MessageRole.ToolUse }
    >;
    const text = renderSubagentSummaryAppendix(parentToolMsg, 'kiro', {
      subagentSummariesById: summaries,
      display: DEFAULT_DISPLAY,
    });
    const plain = stripAnsi(text ?? '');
    expect(plain).toContain('subagent response');
    expect(plain).toContain('response:');
    expect(plain).not.toContain('response summary:');
    expect(plain).toContain('FULLOUTPUTKAS-0');
    expect(plain).toContain('(+5 more lines)');
    expect(plain).not.toContain('FULLOUTPUTKAS-34');

    // Even with the verbose `subagent` filter on, a plain response renders ONLY
    // in the `response:` section — NOT also in a `full output:` section (that
    // double-render was the reported bug). Plain responses aren't raw tool
    // output, so full output: must omit them.
    const verboseBlock = stripAnsi(
      renderSubagentFinalBlock(
        parentToolMsg.content,
        parentToolMsg.result,
        'done',
        undefined,
        summaries.get('parent-1'),
        {
          display: DEFAULT_DISPLAY,
          filtersOverride: ['subagent'],
        }
      )
    );
    expect(verboseBlock).not.toContain('full output:');
    expect(verboseBlock).toContain('response:');
    // The output appears exactly once (in response:, truncated).
    const occurrences = verboseBlock.split('FULLOUTPUTKAS-0').length - 1;
    expect(occurrences).toBe(1);
  });

  test('renders a user-visible late summary appendix for an already-flushed parent', () => {
    const parentMsg = {
      ...parent('parent-1', 'crew-1'),
      content: JSON.stringify({
        task: 'inspect layout and mention response summary: in the prompt',
        stages: [
          {
            name: 'explore-components',
            role: 'explorer',
            prompt_template: 'inspect {task}',
          },
        ],
      }),
      result: { status: 'success', output: 'done' },
    } as MessageType;
    const messages = [parentMsg];
    const sessions = new Map([
      ['sub-chat', session('sub-chat', 'explore-components', 'crew-1')],
    ]);
    const conversations = new Map<string, MessageType[]>([
      [
        'sub-chat',
        [
          summary('summary-chat', {
            contextSummary: 'components digest',
            taskResult: 'read src/components/chat',
          }),
        ],
      ],
    ]);
    const summaries = collectSubagentSummariesByParent(
      messages,
      sessions,
      conversations,
      'kiro'
    );

    const text = renderSubagentSummaryAppendix(
      parentMsg as Extract<MessageType, { role: MessageRole.ToolUse }>,
      'kiro',
      {
        subagentSummariesById: summaries,
        display: DEFAULT_DISPLAY,
      }
    );

    const plain = stripAnsi(text ?? '');
    expect(plain).toContain('response summary:');
    expect(plain).toContain('components digest');
    expect(plain).not.toContain('pipeline:');
    expect(plain).not.toContain('inspect layout');
  });

  test('renders only newly arrived late summaries after earlier summaries were committed', () => {
    const parentMsg = {
      ...parent('parent-1', 'crew-1'),
      content: JSON.stringify({
        task: 'inspect layout',
        stages: [
          {
            name: 'explore-components',
            role: 'explorer',
            prompt_template: 'inspect components',
          },
          {
            name: 'explore-utils-and-parsers',
            role: 'explorer',
            prompt_template: 'inspect utilities',
          },
        ],
      }),
      result: { status: 'success', output: 'done' },
    } as MessageType;
    const messages = [parentMsg];
    const sessions = new Map([
      ['sub-chat', session('sub-chat', 'explore-components', 'crew-1')],
      [
        'sub-layout',
        session('sub-layout', 'explore-utils-and-parsers', 'crew-1'),
      ],
    ]);
    const conversations = new Map<string, MessageType[]>([
      [
        'sub-chat',
        [
          summary('summary-chat', {
            contextSummary: 'components digest',
            taskResult: 'read src/components/chat',
          }),
        ],
      ],
      [
        'sub-layout',
        [
          summary('summary-layout', {
            contextSummary: 'layout digest',
            taskResult: 'read src/components/layout',
          }),
        ],
      ],
    ]);
    const summaries =
      collectSubagentSummariesByParent(
        messages,
        sessions,
        conversations,
        'kiro'
      ).get('parent-1') ?? [];
    const emitted = new Set([subagentSummaryKey(summaries[0]!)]);
    const newlyArrived = summaries.filter(
      (item) => !emitted.has(subagentSummaryKey(item))
    );

    const text = renderSubagentSummaryAppendix(
      parentMsg as Extract<MessageType, { role: MessageRole.ToolUse }>,
      'kiro',
      {
        subagentSummariesById: new Map([['parent-1', summaries]]),
        display: DEFAULT_DISPLAY,
      },
      newlyArrived
    );

    const plain = stripAnsi(text ?? '');
    expect(plain).toContain('response summary:');
    expect(plain).toContain('layout digest');
    expect(plain).not.toContain('components digest');
    expect(plain).not.toContain('pipeline:');
  });

  test('suppresses late summary appendix when responses display is disabled', () => {
    const parentMsg = {
      ...parent('parent-1', 'crew-1'),
      result: { status: 'success', output: 'done' },
    } as MessageType;
    const summaries = [
      {
        stageName: 'explore-components',
        contextSummary: 'components digest',
        taskResult: '',
      },
    ];

    const text = renderSubagentSummaryAppendix(
      parentMsg as Extract<MessageType, { role: MessageRole.ToolUse }>,
      'kiro',
      {
        display: {
          ...DEFAULT_DISPLAY,
          subagent: { ...DEFAULT_DISPLAY.subagent, responses: false },
        },
      },
      summaries
    );

    expect(text).toBeNull();
    expect(
      shouldRenderSubagentResponseSummaries(
        parentMsg as Extract<MessageType, { role: MessageRole.ToolUse }>,
        {
          ...DEFAULT_DISPLAY,
          subagent: { ...DEFAULT_DISPLAY.subagent, responses: false },
        },
        summaries
      )
    ).toBe(false);
  });

  test('renders late KAS response appendix even when responses display is disabled', () => {
    const parentMsg = {
      ...parent('parent-1', 'crew-1'),
      result: { status: 'success', output: 'done' },
    } as MessageType;
    const summaries = [
      {
        stageName: 'respond',
        kind: 'response' as const,
        contextSummary: '',
        taskResult: 'KASRESPONSEPROBE',
      },
    ];
    const display = {
      ...DEFAULT_DISPLAY,
      subagent: { ...DEFAULT_DISPLAY.subagent, responses: false },
    };

    // KAS plain responses are the subagent's real output, not a synthesized
    // summary, so they survive responses:false (Path B parity with render.ts's
    // hasPlainResponses) — otherwise a late-arriving KAS response would be
    // collected but never printed to scrollback.
    expect(
      shouldRenderSubagentResponseSummaries(
        parentMsg as Extract<MessageType, { role: MessageRole.ToolUse }>,
        display,
        summaries
      )
    ).toBe(true);
    const text = renderSubagentSummaryAppendix(
      parentMsg as Extract<MessageType, { role: MessageRole.ToolUse }>,
      'kiro',
      { display },
      summaries
    );
    expect(stripAnsi(text ?? '')).toContain('KASRESPONSEPROBE');
  });

  test('suppresses late summary appendix for failed or cancelled parents', () => {
    const summaries = [
      {
        stageName: 'explore-components',
        contextSummary: 'components digest',
        taskResult: '',
      },
    ];
    const failedParent = {
      ...parent('parent-failed', 'crew-1'),
      result: { status: 'error', error: 'failed' },
    } as MessageType;
    const cancelledParent = {
      ...parent('parent-cancelled', 'crew-1'),
      result: { status: 'cancelled' },
    } as MessageType;

    expect(
      renderSubagentSummaryAppendix(
        failedParent as Extract<MessageType, { role: MessageRole.ToolUse }>,
        'kiro',
        { display: DEFAULT_DISPLAY },
        summaries
      )
    ).toBeNull();
    expect(
      renderSubagentSummaryAppendix(
        cancelledParent as Extract<MessageType, { role: MessageRole.ToolUse }>,
        'kiro',
        { display: DEFAULT_DISPLAY },
        summaries
      )
    ).toBeNull();
  });

  test('suppresses late summary appendix for parents finished without success', () => {
    const parentWithoutResult = {
      ...parent('parent-no-result', 'crew-1'),
    } as MessageType;
    const summaries = [
      {
        stageName: 'explore-components',
        contextSummary: 'components digest',
        taskResult: '',
      },
    ];

    expect(
      renderSubagentSummaryAppendix(
        parentWithoutResult as Extract<
          MessageType,
          { role: MessageRole.ToolUse }
        >,
        'kiro',
        { display: DEFAULT_DISPLAY },
        summaries
      )
    ).toBeNull();
    expect(
      shouldRenderSubagentResponseSummaries(
        parentWithoutResult as Extract<
          MessageType,
          { role: MessageRole.ToolUse }
        >,
        DEFAULT_DISPLAY,
        summaries
      )
    ).toBe(false);
  });

  test('selects and marks only newly arrived summaries for append-only static emission', () => {
    const emitted = new Map<string, Set<string>>();
    const first = {
      stageName: 'explore-components',
      contextSummary: 'components digest',
      taskResult: 'read src/components/chat',
    };
    const second = {
      stageName: 'explore-utils-and-parsers',
      contextSummary: 'layout digest',
      taskResult: 'read src/components/layout',
    };

    expect(
      selectUnemittedSubagentSummaries('parent-1', [first], emitted)
    ).toEqual([first]);
    expect(markSubagentSummariesEmitted('parent-1', [first], emitted)).toBe(0);
    expect(
      selectUnemittedSubagentSummaries('parent-1', [first], emitted)
    ).toEqual([]);
    expect(
      selectUnemittedSubagentSummaries('parent-1', [first, second], emitted)
    ).toEqual([second]);
    expect(markSubagentSummariesEmitted('parent-1', [second], emitted)).toBe(1);
    expect(
      selectUnemittedSubagentSummaries('parent-1', [first, second], emitted)
    ).toEqual([]);
  });

  test('plans late static appendices once after a parent was already flushed', () => {
    const parentMsg = {
      ...parent('parent-1', 'crew-1'),
      result: { status: 'success', output: 'done' },
    } as MessageType;
    const summaries = [
      {
        stageName: 'explore-components',
        contextSummary: 'components digest',
        taskResult: 'read src/components/chat',
      },
    ];
    const emitted = new Map<string, Set<string>>();
    const pushed = new Set(['parent-1']);
    const summariesById = new Map([['parent-1', summaries]]);

    const firstPassEntries = selectPendingSubagentSummaryEntries(
      [parentMsg],
      summariesById,
      pushed,
      emitted,
      DEFAULT_DISPLAY
    );
    const firstPassAppendices = renderPendingSubagentSummaryAppendices(
      firstPassEntries,
      'kiro',
      { display: DEFAULT_DISPLAY }
    );

    expect(firstPassAppendices).toHaveLength(1);
    expect(firstPassAppendices[0]!.id).toBe('parent-1__subagent_summary_0');
    expect(stripAnsi(firstPassAppendices[0]!.text)).toContain(
      'components digest'
    );

    markSubagentSummariesEmitted(
      firstPassAppendices[0]!.parentId,
      firstPassAppendices[0]!.summaries,
      emitted
    );

    const secondPassEntries = selectPendingSubagentSummaryEntries(
      [parentMsg],
      summariesById,
      pushed,
      emitted,
      DEFAULT_DISPLAY
    );

    expect(secondPassEntries).toEqual([]);
  });
});
