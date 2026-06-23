import { isParentSubagentTool } from '../../../types/agent-events.js';
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
  type VerboseDisplayConfig,
} from '../../../lite/verbose.js';
import { isSubagentSummaryToolName } from './SubagentFooter.js';

function parseSummaryTool(
  msg: Extract<MessageType, { role: MessageRole.ToolUse }>,
  stageName: string
): SubagentStageSummary | null {
  if (!isSubagentSummaryToolName(msg.name)) return null;
  try {
    const args = JSON.parse(msg.content);
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
  return `${summary.stageName}\0${summary.contextSummary}\0${summary.taskResult}`;
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

  let activeParentId: string | null = null;
  for (const msg of messages) {
    if (msg.role !== MessageRole.ToolUse) continue;
    const isVisibleParent =
      isParentSubagentTool(msg.name) &&
      (!msg.agentName || msg.agentName === mainAgentName);
    if (isVisibleParent) {
      activeParentId = msg.id;
      if (!out.has(msg.id)) out.set(msg.id, []);
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

  return out;
}

export function shouldRenderSubagentResponseSummaries(
  msg: Extract<MessageType, { role: MessageRole.ToolUse }>,
  display: VerboseDisplayConfig,
  summaries: readonly SubagentStageSummary[]
): boolean {
  if (summaries.length === 0) return false;
  if (!display.subagent.responses) return false;
  if (!msg.isFinished) return false;
  const result = msg.result as ToolResult | undefined;
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
  display: VerboseDisplayConfig
): PendingSubagentSummaryEntry[] {
  const entries: PendingSubagentSummaryEntry[] = [];
  for (const [parentId, summaries] of summariesById) {
    if (summaries.length === 0) continue;
    if (!pushedStaticIds.has(parentId)) continue;
    const newSummaries = selectUnemittedSubagentSummaries(
      parentId,
      summaries,
      emittedByParent
    );
    if (newSummaries.length === 0) continue;
    const parent = messages.find(
      (msg): msg is Extract<MessageType, { role: MessageRole.ToolUse }> =>
        msg.role === MessageRole.ToolUse && msg.id === parentId
    );
    if (
      !parent ||
      !shouldRenderSubagentResponseSummaries(parent, display, newSummaries)
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
  if (!shouldRenderSubagentResponseSummaries(msg, display, summaries)) {
    return null;
  }

  const cols = Math.max(40, renderCtx.termCols ?? 120);
  const lines = renderSubagentResponseSummaryLines(summaries, cols, {
    getStageOutputColor: renderCtx.getStageOutputColor,
    glyphs: renderCtx.glyphs,
  });
  if (lines.length === 0) return null;
  return ['subagent response summary', ...lines].join('\n');
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
