import type { SubagentStageSummary } from '../../../lite/render.js';
import { normalizeSubagentPrompt } from '../../../utils/subagent-display.js';

export interface SubagentStage {
  name?: string;
  role?: string;
  prompt_template?: string;
  depends_on?: string[];
}

export interface ParsedSubagentArgs {
  task: string | null;
  stages: SubagentStage[];
}

/** Unparsable → empty stages (header still renders). */
export function parseSubagentArgs(
  content: string | undefined
): ParsedSubagentArgs {
  if (!content) return { task: null, stages: [] };
  try {
    const args = JSON.parse(content) as {
      task?: unknown;
      stages?: unknown;
    };
    return {
      task: typeof args.task === 'string' ? args.task : null,
      stages: Array.isArray(args.stages)
        ? (args.stages as SubagentStage[])
        : [],
    };
  } catch {
    return { task: null, stages: [] };
  }
}

export function substituteStagePrompt(
  promptTemplate: string | undefined,
  task: string | null
): string | undefined {
  if (!promptTemplate) return promptTemplate;
  const substituted = task
    ? promptTemplate.replace(/\{task\}/g, task)
    : promptTemplate;
  return normalizeSubagentPrompt(substituted);
}

export interface RenderableDigest {
  stageName: string;
  body: string;
}

// contextSummary wins, else taskResult. Rendering owns the configured cap so
// ctrl+o can reveal the complete value.
export function resolveStageDigest(
  summary: SubagentStageSummary
): RenderableDigest | null {
  const ctx = (summary.contextSummary ?? '').trim();
  if (ctx.length > 0) {
    return {
      stageName: summary.stageName,
      body: summary.contextSummary,
    };
  }
  const tr = (summary.taskResult ?? '').trim();
  if (tr.length === 0) return null;
  return {
    stageName: summary.stageName,
    body: summary.taskResult,
  };
}

export function selectSubagentDigests(
  summaries: readonly SubagentStageSummary[]
): {
  responses: RenderableDigest[];
  rawOutput: RenderableDigest[];
  summaries: RenderableDigest[];
} {
  const responses: RenderableDigest[] = [];
  const rawOutput: RenderableDigest[] = [];
  const summaryDigests: RenderableDigest[] = [];

  for (const summary of summaries) {
    const taskResult =
      (summary.taskResult ?? '').trim().length > 0
        ? { stageName: summary.stageName, body: summary.taskResult }
        : null;
    if (summary.kind === 'response') {
      if (taskResult) responses.push(taskResult);
      continue;
    }
    if (taskResult) rawOutput.push(taskResult);
    const digest = resolveStageDigest(summary);
    if (digest) summaryDigests.push(digest);
  }
  return { responses, rawOutput, summaries: summaryDigests };
}
