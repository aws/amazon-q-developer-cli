/**
 * Pure helpers for the lite subagent kill ladder. Extracted so the
 * decision logic can be unit-tested independently of LiteLayout's
 * keypress handler (which is React-bound and harder to drive in tests).
 *
 * The kill ladder itself — first-press-arms, second-press-kills, 2s
 * timer — lives inline in LiteLayout because it's React state plumbing
 * that mirrors the modern TUI's CrewMonitorLayout pattern verbatim.
 */
import { MessageRole, type MessageType } from '../../../stores/app-store.js';

/**
 * After killing a subagent stage, decide whether the currently-pending
 * approval prompt belonged to that stage and should be dismissed.
 *
 * WHY: the killed stage's process is gone the moment `terminateSession`
 * lands; any answer to its pending approval would just fail silently at
 * the backend, so we must drop it rather than leave the user staring at a
 * prompt for a session that no longer exists. Returns true only when the
 * killed stage owns the pending tool (matched via the ToolUse message's
 * `agentName` keyed by `toolCall.toolCallId`).
 */
export function shouldCancelApprovalForKilledStage(
  pendingApproval: { toolCall: { toolCallId: string | null } } | null,
  messages: MessageType[],
  killedStageName: string
): boolean {
  if (!pendingApproval) return false;
  const id = pendingApproval.toolCall.toolCallId;
  if (!id) return false;
  const tool = messages.find(
    (m) => m.role === MessageRole.ToolUse && m.id === id
  );
  if (!tool || tool.role !== MessageRole.ToolUse) return false;
  return tool.agentName === killedStageName;
}
