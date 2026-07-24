import type {
  AgentStreamEvent,
  KiroMeta,
  KiroPipelineStage,
  ToolCallResult,
  ToolCallUpdateEvent,
} from '../types/agent-events.js';
import { AgentEventType, ContentType } from '../types/agent-events.js';

/**
 * Normalizes KAS `invoke_sub_agent` parent tool events into the
 * `orchestrate_subagent` pipeline contract so the existing subagent
 * rendering (SessionTool card, roster, SubagentToolPanel, lite footer,
 * approval tree, crew monitor) applies to delegations without a KAS
 * change.
 *
 * Wire contracts (both produced by KAS `handleSubAgentAction`):
 * - invoke parent card:      `_meta.kiro = { kind: 'agent-subtask', agentSubtaskId }`
 * - orchestrate parent card: `_meta.kiro = { pipeline: { groupId, stages } }`
 * - child events (both):     `_meta.kiro = { agentSubtaskId }` (no `kind`)
 *
 * GROUPING: the model expresses "run these in parallel" as N separate
 * invoke_sub_agent calls (the tool takes one delegation), where
 * orchestrate takes one call with N named stages. To render at parity
 * with orchestrate — ONE "Orchestrating (N agents)" card with N named
 * stage rows, not N stacked single-agent cards — invoke parents that
 * arrive while a previous one is still running are COALESCED into the
 * first (anchor) card's pipeline: their events are rewritten onto the
 * anchor toolCallId, their terminal events become anchor updates until
 * the last member finishes, and each gets a stage label derived from
 * its explanation/prompt (invoke carries no semantic stage names).
 * A delegation that starts after the group fully completed starts a
 * new card, matching a fresh orchestrate call.
 *
 * CRITICAL EXCLUSION: an orchestrate STAGE card carries the exact same
 * `{ kind: 'agent-subtask', agentSubtaskId }` envelope as a standalone
 * invoke parent (the stage IS an InvokeSubAgent call). Stage ids are
 * pre-generated and published in the orchestrate parent's `pipeline`
 * meta before any stage card is emitted, so we learn them from every
 * genuine pipeline meta we see and never claim those cards.
 *
 * META-STRIPPED CARDS: cloud relay/replay emits `…-sub-agent-complete`
 * events whose meta has only messageId/timestamp; cards synthesized
 * from those on the cancel/steer paths carry no subtask identity. A
 * "Sub-agent: <name>" title card with NO identity is claimed (null
 * stage id — rich card, no child correlation); a card with an
 * agentSubtaskId but no kind stays a crew stage wrapper.
 *
 * Hierarchy migration path: KAS emits no parent linkage on nested
 * delegations today, so every stage is flat and {@link projectPipeline}
 * is the ONLY code aware of that flatness. When KAS adds e.g.
 * `parentSubExecutionId`, populate stage parents from it and replace
 * the projection; normalize()'s plumbing stays as-is.
 */

export type InvokeStageStatus = 'pending' | 'running' | 'completed' | 'failed';

export interface InvokeSubagentStageResult {
  name: string;
  status: ToolCallResult['status'];
  output?: unknown;
  error?: string;
}

export interface InvokeSubagentResultEnvelope {
  type: 'invoke_sub_agent_pipeline_results';
  stages: InvokeSubagentStageResult[];
}

export function extractInvokeSubagentResults(
  output: unknown
): InvokeSubagentStageResult[] | null {
  if (!output || typeof output !== 'object') return null;
  const envelope = output as Partial<InvokeSubagentResultEnvelope>;
  return envelope.type === 'invoke_sub_agent_pipeline_results' &&
    Array.isArray(envelope.stages)
    ? envelope.stages
    : null;
}

interface InvokeStage {
  label: string;
  role: string;
  prompt: string | undefined;
  /** Null when the member card arrived meta-stripped. */
  agentSubtaskId: string | null;
  status: InvokeStageStatus;
  result?: ToolCallResult;
}

export interface InvokeSubagentGroup {
  anchorToolCallId: string;
  groupId: string;
  task: string;
  stages: InvokeStage[];
}

const SUBAGENT_TITLE_PREFIX = 'Sub-agent: ';
const GENERIC_SUBAGENT_NAME = 'sub-agent';
const STAGE_LABEL_MAX = 28;
/** Leading boilerplate stripped from prompts when deriving stage labels. */
const LABEL_NOISE =
  /^(you are (a|an) [^.]*\.\s*)?(please\s+)?(research|investigate|analyze|explore|explain|summarize)\b[:\s]*/i;

function stringArg(
  args: Record<string, unknown> | undefined,
  key: string
): string | undefined {
  const value = args?.[key];
  return typeof value === 'string' && value.length > 0 ? value : undefined;
}

function agentNameFrom(
  args: Record<string, unknown> | undefined,
  eventName: string | undefined
): string {
  // invoke_sub_agent rawInput carries the target agent id as `name`;
  // KAS's synthesized input variant carries `subAgentName` instead.
  const fromArgs = stringArg(args, 'name') ?? stringArg(args, 'subAgentName');
  if (fromArgs) return fromArgs;
  if (eventName?.startsWith(SUBAGENT_TITLE_PREFIX)) {
    const parsed = eventName.slice(SUBAGENT_TITLE_PREFIX.length).trim();
    if (parsed.length > 0) return parsed;
  }
  return GENERIC_SUBAGENT_NAME;
}

/**
 * Short human label for a stage row, in place of the semantic stage
 * names an orchestrate call carries. Derived from the delegation's
 * explanation (model's one-line intent) or prompt, boilerplate-stripped
 * and truncated; falls back to the agent id.
 */
function deriveStageLabel(
  args: Record<string, unknown> | undefined,
  agentName: string,
  taken: ReadonlySet<string>
): string {
  const source =
    stringArg(args, 'explanation') ?? stringArg(args, 'prompt') ?? '';
  let label = source.replace(LABEL_NOISE, '').trim();
  const firstSentence = label.split(/[.\n]/, 1)[0] ?? '';
  label = (firstSentence || label).trim();
  if (label.length > STAGE_LABEL_MAX) {
    label = `${label.slice(0, STAGE_LABEL_MAX - 1).trimEnd()}…`;
  }
  if (!label) label = agentName;
  if (!taken.has(label)) return label;
  for (let n = 2; ; n++) {
    const candidate = `${label} #${n}`;
    if (!taken.has(candidate)) return candidate;
  }
}

/**
 * True for the parent card of an individual sub-agent delegation.
 * Child events carry `agentSubtaskId` without `kind`; pipeline parents
 * carry `pipeline` without `kind` — so this shape test is exact (the
 * stage-card ambiguity is resolved by the caller's stage-id registry).
 */
function isInvokeParentMeta(meta: KiroMeta | undefined): meta is KiroMeta & {
  kind: 'agent-subtask';
  agentSubtaskId: string;
} {
  return (
    meta?.kind === 'agent-subtask' &&
    typeof meta.agentSubtaskId === 'string' &&
    meta.agentSubtaskId.length > 0 &&
    meta.pipeline === undefined
  );
}

/**
 * Meta-stripped invoke parent: a "Sub-agent: <name>" title-form card
 * whose meta carries NO subtask identity at all (cloud relay/replay
 * cancel/steer paths). A card that DOES carry an agentSubtaskId
 * (without kind) is a crew stage wrapper and must keep routing as
 * crew activity.
 */
function isMetaStrippedInvokeParent(
  eventName: string | undefined,
  meta: KiroMeta | undefined
): boolean {
  return (
    !!eventName?.startsWith(SUBAGENT_TITLE_PREFIX) &&
    meta?.agentSubtaskId === undefined &&
    meta?.pipeline === undefined
  );
}

function isTerminal(status: InvokeStageStatus): boolean {
  return status === 'completed' || status === 'failed';
}

export class InvokeSubagentPipelineAdapter {
  /**
   * Session-scope gate. The adapter exists to give CLOUD sessions (whose
   * sandbox KAS registers invoke_sub_agent as the top-level delegation
   * tool) parity with local orchestrate rendering. Local sessions never
   * see top-level invoke cards on the happy path — but custom-agent
   * `subagent_<id>` tools and nested delegations emit the same envelope,
   * and claiming those would change what existing local users see. So
   * the adapter is inert until the host marks the session as cloud;
   * while disabled, normalize() is a pure passthrough.
   */
  private enabled = false;
  /** Whether new delegations may start or join a group. */
  private acceptingMembers = true;
  /** The group currently accepting new delegations, if any. */
  private activeGroup: InvokeSubagentGroup | null = null;
  /** ToolCallId (anchor or member) → its group + stage index. */
  private readonly memberIndex = new Map<
    string,
    { group: InvokeSubagentGroup; stage: number }
  >();
  /** agentSubtaskIds seen inside genuine `pipeline` metas: orchestrate
   *  stages (and our own stages' ids, harmlessly). Cards carrying these
   *  ids belong to a crew and are never claimed. */
  private readonly knownStageIds = new Set<string>();

  setEnabled(enabled: boolean): void {
    this.enabled = enabled;
    if (!enabled) this.reset();
  }

  reset(): void {
    this.acceptingMembers = true;
    this.activeGroup = null;
    this.memberIndex.clear();
    this.knownStageIds.clear();
  }

  beginTurn(): void {
    this.reset();
  }

  endTurn(): void {
    this.acceptingMembers = false;
    this.activeGroup = null;
  }

  /**
   * If `event` belongs to an invoke_sub_agent delegation, rewrite it in
   * place onto the group's anchor card in the orchestrate pipeline
   * contract and return the new meta; otherwise return `meta`
   * unchanged. Must run directly after conversion — BEFORE tool-call
   * snapshotting, pipeline interception, and subtask routing.
   *
   * The rewrite removes `kind`/`agentSubtaskId` from the card's meta:
   * an orchestrate parent carries `pipeline` only, and leaving the id
   * on would make routeKasSubtaskEvent classify the card itself as
   * crew activity once its stage is registered — hiding it from the
   * main transcript.
   */
  normalize(
    event: AgentStreamEvent,
    meta: KiroMeta | undefined
  ): KiroMeta | undefined {
    // Inert outside cloud sessions: local rendering is byte-identical
    // to a build without this adapter.
    if (!this.enabled) return meta;
    // Learn stage ids from every genuine pipeline meta FIRST, so an
    // orchestrate parent processed here immediately protects its stages.
    if (meta?.pipeline) {
      for (const stage of meta.pipeline.stages) {
        if (stage.agentSubtaskId) this.knownStageIds.add(stage.agentSubtaskId);
      }
      return meta;
    }

    switch (event.type) {
      case AgentEventType.ToolCall: {
        // Re-emission for an already-claimed card (duplicate update, or
        // base.ts's synthesized Completed-only response ToolCall).
        const existing = this.memberIndex.get(event.id);
        if (existing) {
          return this.rewriteToolCall(event, meta, existing.group);
        }
        if (!this.acceptingMembers) return meta;
        const tagged = isInvokeParentMeta(meta);
        if (tagged && this.knownStageIds.has(meta.agentSubtaskId)) {
          return meta;
        }
        if (!tagged && !isMetaStrippedInvokeParent(event.name, meta)) {
          return meta;
        }

        const agentSubtaskId = tagged ? meta.agentSubtaskId : null;
        const role = agentNameFrom(event.args, event.name);
        const group = this.activeGroupFor();
        const taken = new Set(
          (group?.stages ?? []).map((stage) => stage.label)
        );
        const stage: InvokeStage = {
          label: deriveStageLabel(event.args, role, taken),
          role,
          prompt: stringArg(event.args, 'prompt'),
          agentSubtaskId,
          status: 'running',
        };
        if (agentSubtaskId) this.knownStageIds.add(agentSubtaskId);

        if (group) {
          // Coalesce: this delegation becomes a new stage on the anchor
          // card; the event updates the anchor message in place.
          group.stages.push(stage);
          this.memberIndex.set(event.id, {
            group,
            stage: group.stages.length - 1,
          });
          return this.rewriteToolCall(event, meta, group);
        }

        const created: InvokeSubagentGroup = {
          anchorToolCallId: event.id,
          groupId: `invoke-${event.id}`,
          task:
            stringArg(event.args, 'task') ??
            stringArg(event.args, 'explanation') ??
            stage.prompt ??
            stage.label,
          stages: [stage],
        };
        this.activeGroup = created;
        this.memberIndex.set(event.id, { group: created, stage: 0 });
        return this.rewriteToolCall(event, meta, created);
      }

      case AgentEventType.ToolCallUpdate: {
        const member = this.memberIndex.get(event.id);
        if (!member) return meta;
        event.id = member.group.anchorToolCallId;
        return this.stampPipeline(event, meta, member.group);
      }

      case AgentEventType.ToolCallFinished: {
        const member = this.memberIndex.get(event.id);
        if (!member) return meta;
        const { group, stage } = member;
        // `cancelled` conversions arrive meta-less; treat any
        // non-success terminal as failed so the roster row terminates.
        const stageState = group.stages[stage];
        if (stageState) {
          stageState.status =
            event.result?.status === 'success' ? 'completed' : 'failed';
          stageState.result = event.result;
        }
        this.memberIndex.delete(event.id);

        if (group.stages.every((s) => isTerminal(s.status))) {
          // Last member: finish the anchor with a completion-order-independent
          // result that retains every delegation's terminal payload.
          event.id = group.anchorToolCallId;
          event.result = this.aggregateResult(group);
          if (this.activeGroup === group) this.activeGroup = null;
          return this.stampPipeline(event, meta, group);
        }
        // Other members still running: keep the anchor card open by
        // demoting this terminal to a pipeline-status update. Its result stays
        // on the stage and is restored when the aggregate finishes.
        const demoted = event as unknown as ToolCallUpdateEvent;
        demoted.type = AgentEventType.ToolCallUpdate;
        demoted.id = group.anchorToolCallId;
        demoted.content = { type: ContentType.Text, text: '' };
        delete (demoted as { result?: unknown }).result;
        return this.stampPipeline(demoted, meta, group);
      }

      default:
        return meta;
    }
  }

  /** The group still accepting members, or null. */
  private activeGroupFor(): InvokeSubagentGroup | null {
    const group = this.activeGroup;
    if (!group) return null;
    if (group.stages.every((s) => isTerminal(s.status))) {
      this.activeGroup = null;
      return null;
    }
    return group;
  }

  private aggregateResult(group: InvokeSubagentGroup): ToolCallResult {
    if (group.stages.length === 1) {
      return (
        group.stages[0]?.result ?? {
          status: 'error',
          error: 'Sub-agent finished without a result',
        }
      );
    }

    const stages: InvokeSubagentStageResult[] = group.stages.map((stage) => ({
      name: stage.label,
      status: stage.result?.status ?? 'error',
      ...(stage.result?.status === 'success' && {
        output: stage.result.output,
      }),
      ...(stage.result?.status === 'error' && {
        error: stage.result.error,
        ...(stage.result.output !== undefined && {
          output: stage.result.output,
        }),
      }),
      ...(stage.result?.status === 'cancelled' &&
        stage.result.output !== undefined && { output: stage.result.output }),
    }));
    const output: InvokeSubagentResultEnvelope = {
      type: 'invoke_sub_agent_pipeline_results',
      stages,
    };
    const failed = stages.filter((stage) => stage.status === 'error');
    if (failed.length > 0) {
      return {
        status: 'error',
        error: failed
          .map((stage) => `${stage.name}: ${stage.error || 'failed'}`)
          .join('\n'),
        output,
      };
    }
    if (stages.some((stage) => stage.status === 'cancelled')) {
      return { status: 'cancelled', output };
    }
    return { status: 'success', output };
  }

  private rewriteToolCall(
    event: Extract<AgentStreamEvent, { type: AgentEventType.ToolCall }>,
    meta: KiroMeta | undefined,
    group: InvokeSubagentGroup
  ): KiroMeta {
    event.id = group.anchorToolCallId;
    event.name = 'orchestrate_subagent';
    event.args = this.projectArgs(group);
    return this.stampPipeline(event, meta, group);
  }

  /**
   * Orchestrate-shaped args so downstream parsers work unchanged:
   * SessionTool reads `task` (card label) and `stages.length` (agent
   * count); formatSubagentApprovalLines / renderSubagentFinalBlock read
   * `stages[].{name,role,prompt_template}` for the approval and
   * scrollback pipeline trees.
   */
  private projectArgs(group: InvokeSubagentGroup): Record<string, unknown> {
    return {
      task: group.task,
      stages: group.stages.map((stage) => ({
        name: stage.label,
        role: stage.role,
        ...(stage.prompt !== undefined && { prompt_template: stage.prompt }),
      })),
    };
  }

  private stampPipeline(
    event: AgentStreamEvent & { meta?: { kiro?: KiroMeta } },
    meta: KiroMeta | undefined,
    group: InvokeSubagentGroup
  ): KiroMeta {
    const { kind: _kind, agentSubtaskId: _id, ...rest } = meta ?? {};
    const next: KiroMeta = { ...rest, pipeline: this.projectPipeline(group) };
    event.meta = { kiro: next };
    return next;
  }

  /**
   * Flat projection: the group's stages as one pipeline. This is the
   * ONLY place that encodes flat placement; hierarchical rendering
   * later replaces this projection without touching normalize().
   */
  private projectPipeline(group: InvokeSubagentGroup): {
    groupId: string;
    stages: KiroPipelineStage[];
  } {
    return {
      groupId: group.groupId,
      stages: group.stages.map((stage) => ({
        name: stage.label,
        role: stage.role,
        // 'pending' is unused by invoke stages (they start running),
        // but the type requires the full status union.
        status: stage.status as KiroPipelineStage['status'],
        dependsOn: [],
        agentSubtaskId: stage.agentSubtaskId,
      })),
    };
  }
}
