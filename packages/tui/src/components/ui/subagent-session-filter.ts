import {
  isWorkflowSession,
  type AgentSession,
} from '../../types/multi-session.js';
import { MessageRole, type MessageType } from '../../stores/app-store.js';
import { isParentSubagentTool } from '../../types/agent-events.js';

interface SubagentSessionFilterOptions {
  mainSessionId?: string | null;
  pipelineGroupId?: string;
  pipelineGroupIds?: ReadonlySet<string | undefined>;
}

type ToolUseMessage = Extract<MessageType, { role: MessageRole.ToolUse }>;

export interface ActiveSubagentToolScope {
  key: string;
  parentToolCallId: string;
  parentIndex: number;
  pipelineGroupId?: string;
}

/**
 * Return every unfinished orchestration invocation in message order.
 *
 * Pipeline group is the stable ownership key when KAS provides it. The tool
 * call id keeps ungrouped invocations distinct without conflating stage names.
 */
export function selectActiveSubagentToolScopes(
  messages: readonly MessageType[]
): ActiveSubagentToolScope[] {
  const scopes: ActiveSubagentToolScope[] = [];
  for (let index = 0; index < messages.length; index++) {
    const message = messages[index]!;
    if (
      message.role !== MessageRole.ToolUse ||
      !isParentSubagentTool(message.name) ||
      message.isFinished
    ) {
      continue;
    }
    scopes.push({
      key: message.pipelineGroupId
        ? `group:${message.pipelineGroupId}`
        : `tool:${message.id}`,
      parentToolCallId: message.id,
      parentIndex: index,
      pipelineGroupId: message.pipelineGroupId,
    });
  }
  return scopes;
}

/**
 * Select child tool messages owned by one orchestration invocation.
 *
 * Grouped pipeline messages may interleave freely. For legacy ungrouped
 * invocations, the next parent tool remains the ownership boundary.
 */
export function selectSubagentToolMessagesForScope(
  messages: readonly MessageType[],
  scope: ActiveSubagentToolScope,
  sessionGroupById: ReadonlyMap<string, string | undefined> = new Map()
): ToolUseMessage[] {
  let endIndex = messages.length;
  if (scope.pipelineGroupId === undefined) {
    for (let index = scope.parentIndex + 1; index < messages.length; index++) {
      const message = messages[index]!;
      if (
        message.role === MessageRole.ToolUse &&
        isParentSubagentTool(message.name)
      ) {
        endIndex = index;
        break;
      }
    }
  }

  const owned: ToolUseMessage[] = [];
  for (let index = scope.parentIndex + 1; index < endIndex; index++) {
    const message = messages[index]!;
    if (
      message.role !== MessageRole.ToolUse ||
      isParentSubagentTool(message.name)
    ) {
      continue;
    }
    // Ungrouped scope (engines without KAS pipeline metadata, e.g. the local
    // ACP Rust engine): the parent→next-parent window IS the ownership
    // boundary. Sessions may still carry a `crew-*` group the parent message
    // lacks, so don't require a group match — that mismatch would otherwise
    // drop every child message and leave the footer strip empty.
    if (scope.pipelineGroupId === undefined) {
      owned.push(message);
      continue;
    }
    const messageGroup =
      message.pipelineGroupId ??
      (message.sessionId ? sessionGroupById.get(message.sessionId) : undefined);
    if (messageGroup === scope.pipelineGroupId) owned.push(message);
  }
  return owned;
}

/**
 * Select the sessions owned by one live orchestration tool.
 *
 * When KAS supplies a pipeline group, scope the panel to that exact invocation
 * so agents retained from prior crews cannot reappear.
 */
export function selectSubagentToolSessions(
  sessions: Iterable<AgentSession>,
  options: SubagentSessionFilterOptions = {}
): AgentSession[] {
  const candidates: AgentSession[] = [];
  for (const session of sessions) {
    if (session.id === options.mainSessionId) continue;
    if (session.id.startsWith('pending:')) continue;
    if (session.type !== 'ephemeral') continue;
    if (isWorkflowSession(session)) continue;
    if (
      options.pipelineGroupIds !== undefined &&
      !options.pipelineGroupIds.has(session.group)
    ) {
      continue;
    }
    if (
      options.pipelineGroupIds === undefined &&
      options.pipelineGroupId !== undefined &&
      session.group !== options.pipelineGroupId
    ) {
      continue;
    }
    candidates.push(session);
  }

  // Loop stages can reuse a name inside one pipeline. Keep only the latest
  // iteration (then the newest spawn) without collapsing separate groups.
  const deduped = new Map<string, AgentSession>();
  for (const session of candidates) {
    const key = `${session.group ?? ''}::${session.name}`;
    const existing = deduped.get(key);
    if (
      !existing ||
      (session.loopIteration ?? 0) > (existing.loopIteration ?? 0) ||
      ((session.loopIteration ?? 0) === (existing.loopIteration ?? 0) &&
        session.created.getTime() > existing.created.getTime())
    ) {
      deduped.set(key, session);
    }
  }
  return [...deduped.values()];
}

/**
 * Session-group constraint for the active-subagent footer strip.
 *
 * Returns the set of pipeline groups to scope sessions to, or `undefined` for
 * no group constraint. Engines that don't emit a pipeline group (the local ACP
 * Rust engine) produce ungrouped scopes while their crew sessions still carry a
 * `crew-*` group; constraining to `{undefined}` there would exclude every crew
 * session, so fall back to no constraint. KAS (all scopes grouped) keeps exact
 * per-invocation scoping.
 */
export function activePipelineGroupConstraint(
  scopes: readonly ActiveSubagentToolScope[]
): Set<string | undefined> | undefined {
  if (scopes.some((scope) => scope.pipelineGroupId === undefined)) {
    return undefined;
  }
  return new Set(scopes.map((scope) => scope.pipelineGroupId));
}

/**
 * Sessions that seed the footer rows for one active orchestration scope.
 *
 * Ungrouped scopes (local ACP Rust engine) own every active crew session
 * because the parent tool carries no group to match against; grouped scopes
 * (KAS) match by pipeline group exactly.
 */
export function selectScopeSeedSessions(
  scope: ActiveSubagentToolScope,
  sessions: readonly AgentSession[]
): AgentSession[] {
  if (scope.pipelineGroupId === undefined) return [...sessions];
  return sessions.filter((session) => session.group === scope.pipelineGroupId);
}
