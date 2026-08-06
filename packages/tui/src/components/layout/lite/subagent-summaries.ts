import { isParentSubagentTool } from '../../../types/agent-events.js';
import { extractInvokeSubagentResults } from '../../../utils/invoke-subagent-pipeline.js';
import type { MessageType, ToolResult } from '../../../stores/app-store.js';
import { MessageRole, ToolUseStatus } from '../../../stores/app-store.js';
import type { AgentSession } from '../../../types/multi-session.js';
import {
  renderSubagentResponseSummaryLines,
  type RenderContext,
  type SubagentStageSummary,
} from '../../../lite/render.js';
import {
  getVerboseDisplay,
  shouldShowToolOutput,
  type VerboseDisplayConfig,
} from '../../../lite/verbose.js';
import { subagentSummaryToolKind } from './SubagentFooter.js';
import {
  orderSubagentStageItems,
  parseSubagentStageNames,
} from '../../../utils/subagent-display.js';

function parseSummaryTool(
  msg: Extract<MessageType, { role: MessageRole.ToolUse }>,
  stageName: string
): SubagentStageSummary | null {
  const toolKind = subagentSummaryToolKind(msg.name);
  if (!toolKind) return null;
  try {
    const args = JSON.parse(msg.content);
    const response = typeof args.response === 'string' ? args.response : '';
    if (toolKind === 'subagent_response' && response) {
      return {
        stageName,
        kind: 'response',
        contextSummary: '',
        taskResult: response,
      };
    }
    const contextSummary =
      typeof args.contextSummary === 'string' ? args.contextSummary : '';
    const taskResult =
      typeof args.taskResult === 'string' ? args.taskResult : '';
    if (!contextSummary && !taskResult) return null;
    return { stageName, contextSummary, taskResult };
  } catch {
    return null;
  }
}

function pushSummary(
  out: Map<string, SubagentStageSummary[]>,
  seen: Map<string, Set<string>>,
  parentId: string,
  summary: SubagentStageSummary
): void {
  const key = subagentSummaryKey(summary);
  let parentSeen = seen.get(parentId);
  if (!parentSeen) {
    parentSeen = new Set();
    seen.set(parentId, parentSeen);
  }
  if (parentSeen.has(key)) return;
  parentSeen.add(key);
  let summaries = out.get(parentId);
  if (!summaries) {
    summaries = [];
    out.set(parentId, summaries);
  }
  summaries.push(summary);
}

export function subagentSummaryKey(summary: SubagentStageSummary): string {
  return `${summary.stageName}\0${summary.kind ?? 'summary'}\0${summary.contextSummary}\0${summary.taskResult}`;
}

export function selectUnemittedSubagentSummaries(
  parentId: string,
  summaries: readonly SubagentStageSummary[],
  emittedByParent: ReadonlyMap<string, ReadonlySet<string>>
): SubagentStageSummary[] {
  const emitted = emittedByParent.get(parentId);
  return summaries.filter(
    (summary) => !emitted?.has(subagentSummaryKey(summary))
  );
}

export function markSubagentSummariesEmitted(
  parentId: string,
  summaries: readonly SubagentStageSummary[],
  emittedByParent: Map<string, Set<string>>
): number {
  const emitted = emittedByParent.get(parentId) ?? new Set<string>();
  const previousSize = emitted.size;
  for (const summary of summaries) {
    emitted.add(subagentSummaryKey(summary));
  }
  emittedByParent.set(parentId, emitted);
  return previousSize;
}

export function collectSubagentSummariesByParent(
  messages: readonly MessageType[],
  sessions: ReadonlyMap<string, AgentSession>,
  conversations: ReadonlyMap<string, readonly MessageType[]>,
  mainAgentName?: string | null
): Map<string, SubagentStageSummary[]> {
  const out = new Map<string, SubagentStageSummary[]>();
  const seen = new Map<string, Set<string>>();
  const parentIdByGroup = new Map<string, string>();
  const stageNamesByParent = new Map<string, string[]>();

  let activeParentId: string | null = null;
  for (const msg of messages) {
    if (msg.role !== MessageRole.ToolUse) continue;
    const isVisibleParent =
      isParentSubagentTool(msg.name, msg.origin) &&
      (!msg.agentName || msg.agentName === mainAgentName);
    if (isVisibleParent) {
      activeParentId = msg.id;
      if (!out.has(msg.id)) out.set(msg.id, []);
      stageNamesByParent.set(msg.id, parseSubagentStageNames(msg.content));
      if (msg.pipelineGroupId) parentIdByGroup.set(msg.pipelineGroupId, msg.id);
      continue;
    }
    if (!activeParentId) continue;
    if (!msg.agentName || msg.agentName === mainAgentName) continue;
    const summary = parseSummaryTool(msg, msg.agentName);
    if (summary) pushSummary(out, seen, activeParentId, summary);
  }

  for (const [sessionId, conversation] of conversations) {
    const session = sessions.get(sessionId);
    if (!session?.group) continue;
    const parentId = parentIdByGroup.get(session.group);
    if (!parentId) continue;
    for (const msg of conversation) {
      if (msg.role !== MessageRole.ToolUse) continue;
      const summary = parseSummaryTool(msg, session.name || sessionId);
      if (summary) pushSummary(out, seen, parentId, summary);
    }
  }

  // Invoke parents retain final answers when no child response exists.
  for (const msg of messages) {
    if (msg.role !== MessageRole.ToolUse) continue;
    if (!isParentSubagentTool(msg.name) || !msg.isFinished) continue;
    const result = msg.result;
    if (!result || result.output === undefined) continue;
    const existingResponses = new Set(
      (out.get(msg.id) ?? [])
        .filter((summary) => summary.kind === 'response')
        .map((summary) => summary.stageName)
    );
    const aggregate = extractInvokeSubagentResults(result.output);
    if (aggregate) {
      for (const stage of aggregate) {
        if (
          existingResponses.has(stage.name) ||
          typeof stage.output !== 'string'
        )
          continue;
        const output = stage.output.trim();
        if (!output) continue;
        pushSummary(out, seen, msg.id, {
          stageName: stage.name,
          kind: 'response',
          contextSummary: '',
          taskResult: output,
        });
      }
      continue;
    }
    if (!msg.pipelineGroupId?.startsWith('invoke-')) continue;
    if (result.status !== 'success' || typeof result.output !== 'string')
      continue;
    const output = result.output.trim();
    if (!output) continue;
    let stageName: string | undefined;
    try {
      const args = JSON.parse(msg.content) as {
        stages?: Array<{ name?: string }>;
      };
      if (Array.isArray(args.stages) && args.stages.length === 1) {
        stageName = args.stages[0]?.name;
      }
    } catch {
      // Not pipeline-shaped args — leave stageName unset and skip.
    }
    if (!stageName || existingResponses.has(stageName)) continue;
    pushSummary(out, seen, msg.id, {
      stageName,
      kind: 'response',
      contextSummary: '',
      taskResult: output,
    });
  }

  for (const [parentId, summaries] of out) {
    out.set(
      parentId,
      orderSubagentStageItems(summaries, stageNamesByParent.get(parentId) ?? [])
    );
  }

  return out;
}

export function collectSettledSubagentStagesByParent(
  messages: readonly MessageType[],
  sessions: ReadonlyMap<string, AgentSession>,
  mainAgentName?: string | null
): Map<string, Set<string>> {
  const parentIdByGroup = new Map<string, string>();
  for (const msg of messages) {
    if (
      msg.role === MessageRole.ToolUse &&
      isParentSubagentTool(msg.name) &&
      (!msg.agentName || msg.agentName === mainAgentName) &&
      msg.pipelineGroupId
    ) {
      parentIdByGroup.set(msg.pipelineGroupId, msg.id);
    }
  }

  const settledByParent = new Map<string, Set<string>>();
  for (const [sessionId, session] of sessions) {
    if (
      !session.group ||
      session.status === 'busy' ||
      session.status === 'pending'
    ) {
      continue;
    }
    const parentId = parentIdByGroup.get(session.group);
    if (!parentId) continue;
    const stageName = session.stageInfo?.name || session.name || sessionId;
    const settled = settledByParent.get(parentId) ?? new Set<string>();
    settled.add(stageName);
    settledByParent.set(parentId, settled);
  }
  return settledByParent;
}

export function selectReadySubagentSummaries(
  parent: Extract<MessageType, { role: MessageRole.ToolUse }>,
  summaries: readonly SubagentStageSummary[],
  settledStageNames: ReadonlySet<string> = new Set()
): SubagentStageSummary[] {
  const stageNames = parseSubagentStageNames(parent.content);
  if (stageNames.length === 0) return [...summaries];

  const byStage = new Map<string, SubagentStageSummary[]>();
  for (const summary of summaries) {
    const stage = byStage.get(summary.stageName) ?? [];
    stage.push(summary);
    byStage.set(summary.stageName, stage);
  }

  const ready: SubagentStageSummary[] = [];
  for (const stageName of stageNames) {
    const stageSummaries = byStage.get(stageName);
    if (stageSummaries?.length) {
      ready.push(...stageSummaries);
      continue;
    }
    if (!settledStageNames.has(stageName)) return ready;
  }

  const declared = new Set(stageNames);
  ready.push(
    ...summaries.filter((summary) => !declared.has(summary.stageName))
  );
  return ready;
}

/**
 * Last-args memo over {@link collectSubagentSummariesByParent}, so the N crew
 * cards in one render pass share a single build instead of each re-scanning all
 * messages. Invalidated when any argument's identity changes.
 */
type SummaryArgs = Parameters<typeof collectSubagentSummariesByParent>;
let _summaryMapCache: {
  args: SummaryArgs;
  value: Map<string, SubagentStageSummary[]>;
} | null = null;

export function collectSubagentSummariesByParentCached(
  ...args: SummaryArgs
): Map<string, SubagentStageSummary[]> {
  const c = _summaryMapCache;
  if (c && args.every((a, i) => a === c.args[i])) return c.value;
  const value = collectSubagentSummariesByParent(...args);
  _summaryMapCache = { args, value };
  return value;
}

export function shouldRenderSubagentResponseSummaries(
  msg: Extract<MessageType, { role: MessageRole.ToolUse }>,
  display: VerboseDisplayConfig,
  summaries: readonly SubagentStageSummary[],
  filtersOverride?: readonly string[]
): boolean {
  if (summaries.length === 0) return false;
  const hasPlainResponses = summaries.some((s) => s.kind === 'response');
  if (hasPlainResponses && !shouldShowToolOutput('subagent', filtersOverride)) {
    return false;
  }
  if (!display.subagent.responses && !hasPlainResponses) return false;
  if (!msg.isFinished) return false;
  const result = msg.result as ToolResult | undefined;
  if (hasPlainResponses) return msg.status !== ToolUseStatus.Rejected;
  if (result?.status !== 'success') return false;
  return msg.status !== ToolUseStatus.Rejected;
}

export type PendingSubagentSummaryAppendix = {
  id: string;
  parentId: string;
  text: string;
  summaries: SubagentStageSummary[];
};

export type PendingSubagentSummaryEntry = {
  parent: Extract<MessageType, { role: MessageRole.ToolUse }>;
  parentId: string;
  summaries: SubagentStageSummary[];
  emittedCount: number;
};

export function selectPendingSubagentSummaryEntries(
  messages: readonly MessageType[],
  summariesById: ReadonlyMap<string, readonly SubagentStageSummary[]>,
  pushedStaticIds: ReadonlySet<string>,
  emittedByParent: ReadonlyMap<string, ReadonlySet<string>>,
  display: VerboseDisplayConfig,
  filtersOverride?: readonly string[],
  settledStagesByParent: ReadonlyMap<string, ReadonlySet<string>> = new Map()
): PendingSubagentSummaryEntry[] {
  if (!display.persistOutput) return [];
  const entries: PendingSubagentSummaryEntry[] = [];
  for (const [parentId, summaries] of summariesById) {
    if (summaries.length === 0) continue;
    if (!pushedStaticIds.has(parentId)) continue;
    const parent = messages.find(
      (msg): msg is Extract<MessageType, { role: MessageRole.ToolUse }> =>
        msg.role === MessageRole.ToolUse && msg.id === parentId
    );
    if (!parent) continue;
    const readySummaries = selectReadySubagentSummaries(
      parent,
      summaries,
      settledStagesByParent.get(parentId)
    );
    const newSummaries = selectUnemittedSubagentSummaries(
      parentId,
      readySummaries,
      emittedByParent
    );
    if (newSummaries.length === 0) continue;
    if (
      !shouldRenderSubagentResponseSummaries(
        parent,
        display,
        newSummaries,
        filtersOverride
      )
    ) {
      continue;
    }
    entries.push({
      parent,
      parentId,
      summaries: newSummaries,
      emittedCount: emittedByParent.get(parentId)?.size ?? 0,
    });
  }
  return entries;
}

export function renderSubagentSummaryAppendix(
  msg: Extract<MessageType, { role: MessageRole.ToolUse }>,
  _mainAgentName: string | null | undefined,
  renderCtx: RenderContext,
  summaries = renderCtx.subagentSummariesById?.get(msg.id) ?? []
): string | null {
  const display = renderCtx.display ?? getVerboseDisplay();
  if (renderCtx.isStatic && !display.persistOutput) return null;
  if (
    !shouldRenderSubagentResponseSummaries(
      msg,
      display,
      summaries,
      renderCtx.filtersOverride
    )
  ) {
    return null;
  }

  const cols = Math.max(40, renderCtx.termCols ?? 120);
  // Plain responses render with the input color (matching the prompt's stage
  // name); summaries keep the response-chip color. renderSubagentResponseSummaryLines
  // picks the chip per-stage from kind, so thread both colors.
  const lines = renderSubagentResponseSummaryLines(summaries, cols, {
    getStageInputColor: renderCtx.getStageInputColor,
    getStageOutputColor: renderCtx.getStageOutputColor,
    glyphs: renderCtx.glyphs,
    outputMaxLines: display.outputMaxLines,
    outputMaxChars: display.outputMaxChars,
  });
  if (lines.length === 0) return null;
  const title = summaries.every((summary) => summary.kind === 'response')
    ? 'subagent response'
    : 'subagent response summary';
  return [title, ...lines].join('\n');
}

export function renderPendingSubagentSummaryAppendices(
  entries: readonly PendingSubagentSummaryEntry[],
  mainAgentName: string | null | undefined,
  renderCtx: RenderContext
): PendingSubagentSummaryAppendix[] {
  const appendices: PendingSubagentSummaryAppendix[] = [];
  for (const entry of entries) {
    const text = renderSubagentSummaryAppendix(
      entry.parent,
      mainAgentName,
      renderCtx,
      entry.summaries
    );
    if (!text) continue;
    appendices.push({
      id: `${entry.parentId}__subagent_summary_${entry.emittedCount}`,
      parentId: entry.parentId,
      text,
      summaries: entry.summaries,
    });
  }
  return appendices;
}
