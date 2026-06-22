/**
 * E2E: a blocking crew (`subagent` tool) must not hang when a stage fails.
 *
 * Core bug: when a stage in a dependency graph fails for ANY reason (the failed
 * stage produces no result), the parent's blocking crew tool can hang forever
 * on "Orchestrating". The failed stage's downstream dependents get pruned, but
 * nothing re-fires the parent's group-completion waiter, so the tool never
 * returns. This is independent of why the stage failed.
 *
 * Failure vehicle: we force a hard stage failure by injecting a backend stream
 * error (`ConverseStreamErrorKind::Unknown`), which the agent loop treats as a
 * terminal, non-retryable error. This is a deterministic way to make a subagent
 * stage error through the mock.
 *
 * Note: an empty response (e.g. Bedrock `stopReason=content_filtered`, which
 * reaches the client as metering + metadata only, no content) is NOT a failure
 * - the subagent degrades to its last available message instead of erroring.
 * Case 4 covers that.
 *
 * Cases:
 *   1. Single stage that fails: the crew tool reports an error and the parent
 *      returns to idle.
 *   2. Two-stage DAG where the upstream dependency fails: previously hung; the
 *      fix makes the crew tool fail fast so the parent returns to idle.
 *   3. Sequential same-task crews: a prior crew's failure must not poison a
 *      later crew that reuses the same group key.
 *   4. An empty-response stage degrades to its last message and the crew tool
 *      succeeds rather than failing.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import type { MockStreamItem } from './types/chat-cli';

/**
 * An empty response: the client-side wire shape of a Bedrock content_filtered
 * outcome (metering + metadata only, no content). The subagent degrades to its
 * last available message rather than failing, so this is used to exercise
 * graceful degradation - NOT to fail a stage.
 */
const CONTENT_FILTERED: MockStreamItem[] = [
  {
    kind: 'event',
    data: {
      kind: 'MeteringEvent',
      data: { usage: 0.353, unit: 'credit', unit_plural: 'credits' },
    },
  },
  {
    kind: 'event',
    data: {
      kind: 'MetadataEvent',
      data: { total_tokens: 100, uncached_input_tokens: 95, output_tokens: 5 },
    },
  },
];

/**
 * A hard, terminal stage failure: a mid-stream backend error. A `streamError`
 * surfaces from `recv()` as an error, which the client maps to
 * `StreamErrorKind::Other` - a terminal kind the agent loop does not retry -
 * regardless of the inner `ConverseStreamErrorKind`. `streamError` is a
 * runtime-only MockStreamItem variant absent from the generated TS types,
 * hence the cast.
 */
const HARD_FAILURE = [
  {
    kind: 'streamError',
    data: {
      request_id: null,
      status_code: 500,
      kind: { Unknown: { reason_code: 'TestInducedFailure' } },
    },
  },
] as unknown as MockStreamItem[];

describe('Subagent stage failure in a blocking crew', () => {
  let tc: E2ETestCase | null = null;
  afterEach(async () => {
    await tc?.cleanup();
    tc = null;
  });

  it('single failing stage (hard backend error) — tool errors and parent returns to idle', async () => {
    tc = await E2ETestCase.builder()
      .withTestName('subagent-stage-failure')
      .withGlobalAgentConfig('crew-parent', {
        name: 'crew-parent',
        description: 'Parent agent for crew stage-failure e2e',
        tools: ['*'],
        allowedTools: ['*'],
      })
      .withCliArgs('--agent', 'crew-parent')
      .withTimeout(60000)
      .launch();

    await tc.waitForText('ask a question', 15000);
    await tc.getSessionId();

    // Parent turn: call the subagent tool with a single stage. The stage `role`
    // is the agent name the child session spawns with.
    await tc.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'crew-1',
            name: 'subagent',
            input: JSON.stringify({
              task: 'analyze the data',
              stages: [
                {
                  name: 'analytics',
                  role: 'crew-parent',
                  prompt_template: 'analyze {task}',
                },
              ],
            }),
            stop: true,
          },
        },
      },
    ]);
    await tc.pushSendMessageResponse(null);

    // Kick off the parent turn.
    await tc.sendKeys('run the analytics crew');
    await tc.sleepMs(100);
    await tc.pressEnter();

    // The child session spawns; discover its id.
    const childId = await tc.waitForChildSession();
    console.log(`Discovered child subagent session: ${childId}`);

    // Inject a hard backend error so the stage fails terminally.
    await tc.pushSendMessageResponseForSession(childId, HARD_FAILURE);
    await tc.pushSendMessageResponseForSession(childId, null);

    // After the child errors, the crew tool returns the failure to the parent,
    // and the parent issues a follow-up turn to process that tool result. That
    // follow-up turn needs its own model response; mock it so the only thing
    // that could keep the parent processing is a genuine orchestration hang.
    await tc.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'The analytics stage failed to produce a response.' },
        },
      },
    ]);
    await tc.pushSendMessageResponse(null);

    // Observe behavior for up to ~30s: does the parent return to idle?
    const HANG_WINDOW_MS = 30000;
    let becameIdle = false;
    const start = Date.now();
    while (Date.now() - start < HANG_WINDOW_MS) {
      const store = await tc.getStore();
      if (!store.isProcessing) {
        becameIdle = true;
        break;
      }
      await tc.sleepMs(500);
    }

    const store = await tc.getStore();
    const sessions = store.sessions ?? {};
    const toolMsgs = ((store.messages ?? []) as Array<{
      role: string;
      name?: string;
      result?: { status: string; error?: string };
    }>).filter(m => m.role === 'tool_use');
    const subagentTool = toolMsgs.filter(m => m.name === 'subagent').at(-1);
    console.log('--- OBSERVED STATE ---');
    console.log(`isProcessing: ${store.isProcessing}`);
    console.log(`becameIdle within ${HANG_WINDOW_MS}ms: ${becameIdle}`);
    console.log(`subagent tool result: ${JSON.stringify(subagentTool?.result)}`);
    console.log(`sessions: ${JSON.stringify(sessions, null, 2)}`);
    console.log('--- TERMINAL ---');
    console.log(tc.getSnapshotFormatted());

    // The parent must not hang...
    expect(becameIdle).toBe(true);
    // ...and a failed stage must be surfaced as a tool error, not reported as a
    // successful/empty completion (consistent with the multi-stage failure path).
    expect(subagentTool?.result?.status).toBe('error');
  }, 90000);

  /**
   * Two-stage DAG: stage B depends on stage A. Stage A's response is
   * content-filtered. Because B carries a `depends_on`, it is NOT spawned
   * immediately — it sits in the group's pending_stages until A terminates and
   * `trigger_pending_stages` advances the DAG. This probes the branch where a
   * content-filtered upstream stage could leave a downstream stage stranded in
   * pending forever, so the parent's WaitForGroupCompletion waiter never fires.
   */
  it('failed upstream DAG stage — downstream stage + parent must not hang', async () => {
    tc = await E2ETestCase.builder()
      .withTestName('subagent-stage-failure-dag')
      .withGlobalAgentConfig('crew-parent', {
        name: 'crew-parent',
        description: 'Parent agent for crew content-filter DAG e2e',
        tools: ['*'],
        allowedTools: ['*'],
      })
      .withCliArgs('--agent', 'crew-parent')
      .withTimeout(60000)
      .launch();

    await tc.waitForText('ask a question', 15000);
    await tc.getSessionId();

    // Parent turn: subagent call with two stages, B depends on A.
    await tc.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'crew-dag-1',
            name: 'subagent',
            input: JSON.stringify({
              task: 'analyze the data',
              stages: [
                { name: 'stage-a', role: 'crew-parent', prompt_template: 'gather {task}' },
                {
                  name: 'stage-b',
                  role: 'crew-parent',
                  prompt_template: 'summarize {task}',
                  depends_on: ['stage-a'],
                },
              ],
            }),
            stop: true,
          },
        },
      },
    ]);
    await tc.pushSendMessageResponse(null);

    await tc.sendKeys('run the analytics crew');
    await tc.sleepMs(100);
    await tc.pressEnter();

    // Stage A spawns first (no deps). Fail it hard.
    const stageA = await tc.waitForChildSession();
    console.log(`Discovered stage A: ${stageA}`);
    await tc.pushSendMessageResponseForSession(stageA, HARD_FAILURE);
    await tc.pushSendMessageResponseForSession(stageA, null);

    // If the DAG advances, stage B spawns after A terminates. Content-filter it
    // too so it also fails gracefully. If B never appears, the DAG is stuck —
    // a real hang.
    const stageB = await tc.waitForNewChildSession(new Set([stageA]));
    console.log(`Discovered stage B: ${stageB ?? 'NONE (DAG did not advance)'}`);
    if (stageB) {
      for (let i = 0; i < 4; i++) {
        await tc.pushSendMessageResponseForSession(stageB, CONTENT_FILTERED);
        await tc.pushSendMessageResponseForSession(stageB, null);
      }
    }

    // Parent follow-up turn after the group resolves.
    await tc.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Both stages failed to produce a response.' },
        },
      },
    ]);
    await tc.pushSendMessageResponse(null);

    const HANG_WINDOW_MS = 30000;
    let becameIdle = false;
    const start = Date.now();
    while (Date.now() - start < HANG_WINDOW_MS) {
      const store = await tc.getStore();
      if (!store.isProcessing) {
        becameIdle = true;
        break;
      }
      await tc.sleepMs(500);
    }

    const store = await tc.getStore();
    const sessions = store.sessions ?? {};
    console.log('--- OBSERVED STATE (DAG) ---');
    console.log(`stageB spawned: ${stageB !== null}`);
    console.log(`isProcessing: ${store.isProcessing}`);
    console.log(`becameIdle within ${HANG_WINDOW_MS}ms: ${becameIdle}`);
    console.log(`sessions: ${JSON.stringify(sessions, null, 2)}`);
    console.log('--- TERMINAL ---');
    console.log(tc.getSnapshotFormatted());

    // Fail-fast contract: stage-b's dependency failed, so it must NOT spawn,
    // and the parent's crew tool must return (an error) rather than hang.
    expect(stageB).toBeNull();
    expect(becameIdle).toBe(true);
  }, 120000);

  /**
   * Regression: a failed stage must not poison a later crew that reuses the
   * same task. The orchestration group is keyed by `crew-<task prefix>`, so two
   * sequential crews with the same task share a group key. A stage failure must
   * not leave behind state that makes the next same-task crew fail without
   * running.
   *
   * Crew 1 (task T): single stage fails (content_filtered).
   * Crew 2 (task T, same key): single stage succeeds.
   * The crew-2 tool must complete successfully — not inherit crew-1's failure.
   */
  it('prior stage failure must not poison a later same-task crew', async () => {
    const TASK = 'analyze the data';
    const subagentCall = (toolUseId: string): MockStreamItem[] => [
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: toolUseId,
            name: 'subagent',
            input: JSON.stringify({
              task: TASK,
              stages: [{ name: 'stage-a', role: 'crew-parent', prompt_template: 'do {task}' }],
            }),
            stop: true,
          },
        },
      },
    ];
    const assistant = (text: string): MockStreamItem[] => [
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: text } } },
    ];

    tc = await E2ETestCase.builder()
      .withTestName('subagent-stage-failure-poison')
      .withGlobalAgentConfig('crew-parent', {
        name: 'crew-parent',
        description: 'Parent agent for crew poison-state e2e',
        tools: ['*'],
        allowedTools: ['*'],
      })
      .withCliArgs('--agent', 'crew-parent')
      .withTimeout(60000)
      .launch();

    await tc.waitForText('ask a question', 15000);
    await tc.getSessionId();

    // ---- Crew 1: task T, single stage that fails (hard backend error). ----
    await tc.pushSendMessageResponse(subagentCall('crew-1'));
    await tc.pushSendMessageResponse(null);
    await tc.sendKeys('run the crew');
    await tc.sleepMs(100);
    await tc.pressEnter();

    const stage1 = await tc.waitForChildSession();
    console.log(`Crew 1 stage: ${stage1}`);
    await tc.pushSendMessageResponseForSession(stage1, HARD_FAILURE);
    await tc.pushSendMessageResponseForSession(stage1, null);
    // Parent follow-up after crew 1 returns.
    await tc.pushSendMessageResponse(assistant('crew 1 done'));
    await tc.pushSendMessageResponse(null);
    await tc.waitForIdle(30000);

    // ---- Crew 2: SAME task (same group key), single stage that succeeds. ----
    await tc.pushSendMessageResponse(subagentCall('crew-2'));
    await tc.pushSendMessageResponse(null);
    await tc.sendKeys('run the crew again');
    await tc.sleepMs(100);
    await tc.pressEnter();

    const stage2 = await tc.waitForNewChildSession(new Set([stage1]));
    console.log(`Crew 2 stage: ${stage2 ?? 'NONE'}`);
    // Drive the stage to a successful completion. With no explicit summary tool
    // call, internal_prompt sends a failsafe prompt and then derives the result
    // from the final message, so push a couple of content-bearing turns.
    if (stage2) {
      for (let i = 0; i < 3; i++) {
        await tc.pushSendMessageResponseForSession(stage2, assistant('stage 2 result'));
        await tc.pushSendMessageResponseForSession(stage2, null);
      }
    }
    // Parent follow-up after crew 2 returns.
    await tc.pushSendMessageResponse(assistant('crew 2 done'));
    await tc.pushSendMessageResponse(null);
    await tc.waitForIdle(30000);

    const store = await tc.getStore();
    const toolMsgs = ((store.messages ?? []) as Array<{
      role: string;
      name?: string;
      result?: { status: string; error?: string };
    }>).filter(m => m.role === 'tool_use');
    console.log(
      'crew tool results:',
      JSON.stringify(
        toolMsgs.map(m => ({ name: m.name, status: m.result?.status, error: m.result?.error })),
        null,
        2,
      ),
    );

    // Crew 2 must have actually spawned a fresh stage and its tool must SUCCEED.
    // With the poison bug, crew 2 inherits crew 1's failed group state and its
    // tool returns an error without ever running the new stage. We assert on
    // crew 2 specifically (the last subagent tool message) so this holds
    // regardless of how a single failed stage (crew 1) is reported.
    const subagentTools = toolMsgs.filter(m => m.name === 'subagent');
    const crew2Tool = subagentTools.at(-1);
    expect(stage2).not.toBeNull();
    expect(subagentTools.length).toBe(2);
    expect(crew2Tool?.result?.status).toBe('success');
  }, 150000);

  /**
   * An empty response is not a stage failure: the subagent degrades to its last
   * available message and the crew tool succeeds. The exact fallback text is
   * unit-tested in subagent_tool.rs; here we assert the integration outcome.
   */
  it('empty-response stage degrades and the crew tool succeeds', async () => {
    tc = await E2ETestCase.builder()
      .withTestName('subagent-stage-failure-degrade')
      .withGlobalAgentConfig('crew-parent', {
        name: 'crew-parent',
        description: 'Parent agent for crew empty-response degrade e2e',
        tools: ['*'],
        allowedTools: ['*'],
      })
      .withCliArgs('--agent', 'crew-parent')
      .withTimeout(60000)
      .launch();

    await tc.waitForText('ask a question', 15000);
    await tc.getSessionId();

    await tc.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'ToolUseEvent',
          data: {
            tool_use_id: 'crew-degrade-1',
            name: 'subagent',
            input: JSON.stringify({
              task: 'summarize the data',
              stages: [{ name: 'stage-a', role: 'crew-parent', prompt_template: 'do {task}' }],
            }),
            stop: true,
          },
        },
      },
    ]);
    await tc.pushSendMessageResponse(null);

    await tc.sendKeys('run the crew');
    await tc.sleepMs(100);
    await tc.pressEnter();

    const childId = await tc.waitForChildSession();
    // Empty response, and the retry is empty too — the stage must degrade, not fail.
    for (let i = 0; i < 2; i++) {
      await tc.pushSendMessageResponseForSession(childId, CONTENT_FILTERED);
      await tc.pushSendMessageResponseForSession(childId, null);
    }

    // Parent follow-up after the crew returns.
    await tc.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: 'done' } } },
    ]);
    await tc.pushSendMessageResponse(null);

    await tc.waitForIdle(30000);

    const store = await tc.getStore();
    const subagentTool = ((store.messages ?? []) as Array<{
      role: string;
      name?: string;
      result?: { status: string; error?: string };
    }>)
      .filter(m => m.role === 'tool_use' && m.name === 'subagent')
      .at(-1);
    console.log(`degrade subagent tool result: ${JSON.stringify(subagentTool?.result)}`);

    // An empty response degrades to a (possibly empty) result; the crew tool
    // must SUCCEED rather than fail the stage.
    expect(subagentTool?.result?.status).toBe('success');
  }, 90000);
});
