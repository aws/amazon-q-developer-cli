import { MessageRole, type MessageType } from '../../../stores/app-store.js';
import { needsLeadingBlankByRole } from '../../../lite/blank-rules.js';

/**
 * Cap on rendered history rows when resuming in lite mode. Replay keeps the
 * FULL session in `messages` (context/tool-result lookups stay correct); the
 * cap only clamps how many rows paint into <Static> via
 * `lite.staticSkipBefore = max(0, messages.length - cap)`. It is a static
 * lower bound, not a sliding window, so live turns past the cap still render
 * at the user's configured verbosity.
 */
export const LITE_HISTORY_RENDER_CAP = 70;

/**
 * Inner tool calls inside a subagent stage carry an agentName != main agent
 * (resolved in app-store.ts from a non-main sessionId). The lite log hides
 * them — only the parent `subagent` tool + the final-result block surface.
 * Callers without `mainAgentName` (boot, tests) get everything visible.
 */
export function isInnerSubagentTool(
  msg: MessageType,
  mainAgentName?: string | null
): boolean {
  if (msg.role !== MessageRole.ToolUse) return false;
  if (!msg.agentName) return false;
  if (!mainAgentName) return false;
  return msg.agentName !== mainAgentName;
}

/**
 * Index of the first still-unfinished (non-inner) tool, or -1 if none — the
 * settled/in-flight boundary shared by computeActiveToolBatchIds and
 * selectStaticEligible so they agree on what's withheld from <Static>.
 *
 * Front-scan, never walk-from-end: rejecting one tool of a parallel batch with
 * a note finishes it while its approved sibling stays unfinished, with the note
 * as a trailing User bubble. A walk-from-end breaks at that bubble and misses
 * the unfinished sibling, so it reorders/vanishes until turn end.
 */
export function firstUnfinishedToolIndex(
  messages: MessageType[],
  mainAgentName?: string | null
): number {
  for (let i = 0; i < messages.length; i++) {
    const m = messages[i]!;
    if (m.role !== MessageRole.ToolUse) continue;
    if (isInnerSubagentTool(m, mainAgentName)) continue;
    if (!m.isFinished) return i;
  }
  return -1;
}

/**
 * Ids of the in-flight tool batch: every ToolUse from the first still-unfinished
 * tool onward. Empty when the whole run is done.
 *
 * Prefix-flush, not per-tool: <Static> is a monotonic by-index cursor, but
 * tools can finish out of creation order. Holding everything from the first
 * unfinished tool keeps creation order in scrollback — otherwise a
 * later-but-already-done tool would bake into static at an index its
 * predecessor still needs. Inner subagent tools are skipped (invisible to the
 * layout, and they don't break the run).
 */
export function computeActiveToolBatchIds(
  messages: MessageType[],
  mainAgentName?: string | null
): Set<string> {
  const firstUnfinished = firstUnfinishedToolIndex(messages, mainAgentName);
  if (firstUnfinished === -1) return new Set<string>();
  const ids = new Set<string>();
  for (let i = firstUnfinished; i < messages.length; i++) {
    const m = messages[i]!;
    if (m.role !== MessageRole.ToolUse) continue;
    if (isInnerSubagentTool(m, mainAgentName)) continue;
    ids.add(m.id);
  }
  return ids;
}

/**
 * Messages that should appear in <Static> right now. Still-streaming rows and
 * the in-flight tool batch stay in the live region; tools are de-duped by id.
 *
 * `hideThinkingContent` (== showThinkingContent === false): an empty-content
 * Model with a populated `thinking` field renders the bordered thinking block,
 * so it's kept by default. But when thinking display is off it renders to '',
 * and an eligible empty row makes the LiteLayout delta walk bake a phantom
 * blank gap into <Static>; drop it so the boundary stays compact.
 */
export function selectStaticEligible(
  messages: MessageType[],
  isProcessing: boolean,
  activeBatch?: Set<string>,
  mainAgentName?: string | null,
  hideThinkingContent = false
): MessageType[] {
  const batch =
    activeBatch ?? computeActiveToolBatchIds(messages, mainAgentName);

  // Withhold EVERYTHING at/after the first unfinished tool — not just tools but
  // any trailing row (e.g. a SteeringConsumed note). Admitting such a row would
  // advance LiteLayout's forward-only cursor past the slot the tool reclaims on
  // settle, stranding it out of <Static>. Hold the whole tail so it flushes in
  // creation order regardless of event order.
  const firstUnfinishedToolIdx = firstUnfinishedToolIndex(
    messages,
    mainAgentName
  );

  const eligible: MessageType[] = [];
  const seenToolIds = new Set<string>();
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i]!;
    // Withhold the unsettled trailing region (see firstUnfinishedToolIdx).
    // Inner subagent tools are exempt — they're filtered out below regardless
    // and never reach <Static>, so they can't strand anything.
    if (
      firstUnfinishedToolIdx !== -1 &&
      i >= firstUnfinishedToolIdx &&
      !isInnerSubagentTool(msg, mainAgentName)
    ) {
      continue;
    }
    if (
      msg.role === MessageRole.Model &&
      isProcessing &&
      i === messages.length - 1 &&
      !msg.standalone
    ) {
      continue;
    }
    // Empty Model (only Thought events arrived): drop unless thinking is
    // shown AND populated, else its leading-blank prefix bakes a phantom gap.
    // See docblock above.
    if (msg.role === MessageRole.Model && !msg.content.trim()) {
      if (hideThinkingContent) continue;
      if (!msg.thinking || !msg.thinking.trim()) continue;
    }
    if (msg.role === MessageRole.ToolUse) {
      if (isInnerSubagentTool(msg, mainAgentName)) continue;
      if (!msg.isFinished) continue;
      if (batch.has(msg.id)) continue;
      if (seenToolIds.has(msg.id)) continue;
      seenToolIds.add(msg.id);
    }
    eligible.push(msg);
  }
  return eligible;
}

/**
 * Leading blank between two adjacent static items? Delegates to the shared
 * {@link needsLeadingBlankByRole} so the live region + /verbosity preview
 * stay in lockstep. New roles go in `blank-rules.ts`, not here.
 */
export function needsLeadingBlank(
  prev: MessageType,
  next: MessageType
): boolean {
  return needsLeadingBlankByRole(prev.role, next.role);
}
