import type { TestCase } from '../../src/test-utils/TestCase';
import { AgentEventType } from '../../src/types/agent-events';

export interface SubagentStage {
  toolId: string;
  /** Tool name (e.g. 'Read', 'Shell'). */
  name: string;
  /** Tool kind (default 'read'). */
  kind?: string;
  args: Record<string, unknown>;
  sessionId: string;
}

export interface SeedSubagentOpts {
  parentId: string;
  prompt: string;
  stages: SubagentStage[];
  pipeline?: string;
  /**
   * Sessions to seed via mockAddSession AFTER the prompt is in flight. The
   * parent `subagent` ToolCall handler wipes pre-existing ephemeral sessions
   * (app-store SESSION_TOOL_NAMES), so anything seeded before the prompt is
   * clobbered — these are added once isProcessing=true.
   */
  addSessionsAfter?: Array<{
    id: string;
    name: string;
    status?: 'busy' | 'pending' | 'terminated';
  }>;
  /** Settle after the prompt before seeding sessions (default 300). */
  postPromptMs?: number;
  /** Settle after seeding sessions (default 150 when addSessionsAfter set). */
  postSessionMs?: number;
}

/**
 * Seed an unfinished parent `subagent` ToolCall + N stage ToolCalls, submit
 * the prompt, then optionally seed stage sessions. The parent stays unfinished
 * to gate the activeSubagents memo; stage tool calls carry a sessionId so they
 * resolve to per-stage agent names.
 */
export async function seedSubagentPipeline(
  tc: TestCase,
  opts: SeedSubagentOpts
): Promise<void> {
  await tc.mockSessionUpdate({
    type: AgentEventType.ToolCall,
    id: opts.parentId,
    name: 'subagent',
    args: { pipeline: opts.pipeline ?? 'test-pipeline' },
  });
  for (const stage of opts.stages) {
    await tc.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: stage.toolId,
      name: stage.name,
      kind: (stage.kind ?? 'read') as any,
      args: stage.args,
      sessionId: stage.sessionId,
    });
  }
  await tc.typeAndSubmit(opts.prompt);
  await tc.sleepMs(opts.postPromptMs ?? 300);
  if (opts.addSessionsAfter) {
    for (const s of opts.addSessionsAfter) {
      await tc.mockAddSession({
        id: s.id,
        name: s.name,
        status: s.status ?? 'busy',
      });
    }
    await tc.sleepMs(opts.postSessionMs ?? 150);
  }
}
