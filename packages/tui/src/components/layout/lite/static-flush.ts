import { MessageRole, type MessageType } from '../../../stores/app-store.js';
import { needsLeadingBlankByRole } from '../../../lite/blank-rules.js';

/**
 * Hard cap on rendered history rows when resuming a session in lite mode.
 * Replay always populates `messages` with the FULL session so context-window
 * accounting and tool-result lookups stay correct; the cap only clamps how
 * many of those rows actually paint into <Static>. Implemented via
 * `liteStaticSkipBefore = max(0, messages.length - cap)` after a resume,
 * which is the same bookmark mechanism tui→lite uses to suppress
 * already-rendered scrollback.
 *
 * Resumed rows render at the user's configured verbosity — a forced lean
 * override hid the per-message density a returning user expects from
 * /verbose. Live turns appended past the cap render normally and stay
 * visible because the bookmark is a static lower bound, not a sliding window.
 */
export const LITE_HISTORY_RENDER_CAP = 70;

/**
 * Inner tool calls running INSIDE a subagent stage carry an agentName
 * different from the main agent (resolved in app-store.ts when a tool call
 * arrives with a non-main sessionId). The lite chat log + live region hide
 * these — only the parent `subagent` tool itself surfaces, and a single
 * final-result block lands once the pipeline is done. Everything in this
 * module treats them as if they don't exist.
 *
 * Pass `mainAgentName` from the layout (currentAgent?.name). Callers that
 * don't have it yet (boot, tests) get the legacy behavior with everything
 * visible.
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
 * The "active tool batch" is the trailing run of consecutive ToolUse messages
 * at the end of the message list, starting from the FIRST still-unfinished
 * tool. Finished tools earlier in the run (the contiguous "done prefix")
 * flush to <Static> as soon as their predecessors are done — so a sequence
 * of fast sequential tools doesn't pile up visually in the live region.
 *
 * Why prefix-flush instead of per-tool flush: twinki's <Static> is a
 * monotonic by-index cursor. Tool execution order can differ from creation
 * order (parallel completion), but scrollback must show creation order. If
 * tool B finishes before tool A, we hold B in the live region until A is
 * also done — otherwise B would land in static at an index where A would
 * later need to live, scrambling the order or losing A's row entirely.
 *
 * Returns a Set of message ids belonging to the in-flight batch — empty when
 * every tool in the trailing run is finished (the whole run flushes).
 *
 * Inner subagent tool calls are skipped entirely: they neither belong to the
 * batch nor break the trailing run. From the layout's perspective they don't
 * exist.
 */
export function computeActiveToolBatchIds(
  messages: MessageType[],
  mainAgentName?: string | null
): Set<string> {
  const skip = (m: MessageType): boolean =>
    isInnerSubagentTool(m, mainAgentName);
  // Walk from the end to find the trailing run of (non-inner) tool messages.
  let runStart = messages.length;
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages[i]!;
    if (m.role !== MessageRole.ToolUse) break;
    if (skip(m)) continue;
    runStart = i;
  }
  // Find the first unfinished tool inside that run. Tools before it are the
  // "done prefix" — they belong in static, not the live region.
  let firstUnfinished = -1;
  for (let i = runStart; i < messages.length; i++) {
    const m = messages[i]!;
    if (m.role !== MessageRole.ToolUse) continue;
    if (skip(m)) continue;
    if (!m.isFinished) {
      firstUnfinished = i;
      break;
    }
  }
  if (firstUnfinished === -1) return new Set<string>();
  const ids = new Set<string>();
  for (let i = firstUnfinished; i < messages.length; i++) {
    const m = messages[i]!;
    if (m.role !== MessageRole.ToolUse) continue;
    if (skip(m)) continue;
    ids.add(m.id);
  }
  return ids;
}

/**
 * Pick the messages that should appear in <Static> right now. Messages still
 * streaming, or that belong to the in-flight tool batch, stay in the live
 * region until they're settled. Tool messages are also de-duplicated by id.
 *
 * `hideThinkingContent` corresponds to `showThinkingContent === false`. The
 * empty-Model carve-out below normally keeps a `content: ''` row when its
 * `thinking` field is populated so the persisted thinking block can render
 * (renderMessageToText turns `thinking` into a bordered scrollback section).
 * When thinking display is off the renderer returns `''` for that row, but
 * eligibility would still include it — the LiteLayout delta walk then bakes
 * a `'\n'` leading-blank prefix on top of the empty render, pinning a
 * phantom 2-row gap into <Static>. The next message also stamps its own
 * blank against the empty model as `prevMsg`, so the user sees 3 blank
 * visual rows between sections instead of one. Dropping the row here keeps
 * the boundary compact when thinking display is off.
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
  const eligible: MessageType[] = [];
  const seenToolIds = new Set<string>();
  for (let i = 0; i < messages.length; i++) {
    const msg = messages[i]!;
    if (
      msg.role === MessageRole.Model &&
      isProcessing &&
      i === messages.length - 1 &&
      !msg.standalone
    ) {
      continue;
    }
    // Empty Model rows happen when only Thought events arrived between tools
    // — flushContentToStore pushes a Model with content='' so the thinking
    // payload is preserved, but renderAgentMessage returns '' for it. The
    // empty static item still carries a leading-blank prefix per
    // needsLeadingBlank, which split('\n') turns into two blank rows; the
    // next item adds another. Drop them here so neighboring tool/user/system
    // boundaries collapse cleanly.
    //
    // Exception: when reasoning content rendering is on, a Model row with no
    // spoken text but a populated `thinking` field still has something to
    // surface — the persisted thinking block. Keep those rows in eligible so
    // renderMessageToText can emit the bracketed reasoning section. When the
    // user has turned thinking display off (minimal preset, /verbose menu),
    // there's nothing left to render — drop them so the renderer never gets
    // a chance to bake a phantom blank row.
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
 * Visual-section boundary: should we insert a leading blank line between two
 * adjacent static items? Mirrors the rule used by the live region and the
 * /verbosity preview pane — all three call sites delegate to the shared
 * {@link needsLeadingBlankByRole} helper in `lite/blank-rules.ts` so they
 * stay in lockstep without hand-sync comments.
 *
 * Adding a new role means editing `blank-rules.ts`, not this file.
 */
export function needsLeadingBlank(
  prev: MessageType,
  next: MessageType
): boolean {
  return needsLeadingBlankByRole(prev.role, next.role);
}

/**
 * Format the per-turn credits/time trailer row with its leading blank baked
 * in (rule 5: blank BEFORE the trailer). Pure helper — `rendered` is already
 * styled by the caller (typically `chalk.dim('  …summary…')`); this just
 * prepends the section break so the trailer doesn't glue to the previous
 * row.
 *
 * The trailer is emitted by computeTurnSummaryInsertions outside the main
 * eligible loop, so it doesn't pass through needsLeadingBlank — this is
 * where its spacing rule lives. There is always at least the turn's User
 * message preceding the trailer (see computeTurnSummaryInsertions), so
 * unconditionally prepending '\n' is safe.
 */
export function formatTurnSummaryRow(rendered: string): string {
  return '\n' + rendered;
}

/**
 * For each turn-owning user message in `eligible`, compute the array index
 * (relative to the original eligible list) at which that turn's summary
 * trailer should be slotted. The summary lands BEFORE the eligible message
 * at the returned index — i.e. immediately after the turn's last
 * conversational row (Model / ToolUse) and before any later non-turn row
 * (System announcement, next User message).
 *
 * Map value semantics: `eligible.length` means "tail" — append after every
 * eligible item. Any value < eligible.length means "insert before
 * eligible[value]".
 *
 * The `isProcessing` gate suppresses the tail emission while the turn is
 * still in flight; mid-stream we don't have the metering / timing yet.
 *
 * Why this matters: Twinki's <Static> is a monotonic by-index cursor. Once
 * the trailer has been printed at index P, every subsequent render must
 * keep the trailer at index P or earlier; otherwise the cursor sees a
 * "new" item at the trailer's new (later) index and re-emits the line.
 * Locking the trailer to the END of its turn's content (rather than the
 * trailing index of the whole items array) is what keeps that invariant.
 */
export function computeTurnSummaryInsertions(
  eligible: MessageType[],
  turnSummaries: Map<string, string>,
  isProcessing: boolean
): Map<string, number> {
  const out = new Map<string, number>();
  let currentTurnUserId: string | null = null;
  for (let i = 0; i < eligible.length; i++) {
    const msg = eligible[i]!;
    if (
      currentTurnUserId &&
      (msg.role === MessageRole.User || msg.role === MessageRole.System) &&
      turnSummaries.has(currentTurnUserId) &&
      !out.has(currentTurnUserId)
    ) {
      out.set(currentTurnUserId, i);
    }
    if (msg.role === MessageRole.User) {
      currentTurnUserId = msg.id;
    }
  }
  if (
    !isProcessing &&
    currentTurnUserId &&
    turnSummaries.has(currentTurnUserId) &&
    !out.has(currentTurnUserId)
  ) {
    out.set(currentTurnUserId, eligible.length);
  }
  return out;
}

/**
 * Decide whether a `liteScrollbackClearToken` bump should push a static
 * "now in lite" session-anchor banner at index 0 of the freshly-emptied
 * <Static> items array.
 *
 * Two arms:
 *   1. `messages.some(User)` — the messages list still carries chat
 *      content from the prior session. Covers tui→lite mid-session,
 *      /chat <id> load with history, and replay past the lite history
 *      cap. The token bump preserves messages while wiping
 *      staticItemsRef, so we have a User row to gate on.
 *   2. `hadPriorStaticContent` — at the moment the token bump fires,
 *      the layout had already committed any rows to staticItemsRef.
 *      Covers /chat new mid-session, where `resetMessages` empties
 *      messages BEFORE bumping the token (so messages.some(User) is
 *      false by the time the layout body runs) but the layout had
 *      committed rows from the prior session. Without this arm the
 *      live-region banner re-renders against terminal scrollback that
 *      still holds the prior session's static rows above it, and the
 *      user sees the KIRO art twice on screen.
 *
 * Cold-boot truly-empty sessions return false on both arms — the
 * caller should rely on the live-region banner there so a resize
 * reflows it cleanly.
 *
 * The Boolean cast on hadPriorStaticContent is a defensive hedge in
 * case a future caller passes 0/1 from a length read.
 */
export function shouldPushSwapWithContentBanner(
  messages: MessageType[],
  hadPriorStaticContent: boolean
): boolean {
  return (
    Boolean(hadPriorStaticContent) ||
    messages.some((m) => m.role === MessageRole.User)
  );
}
