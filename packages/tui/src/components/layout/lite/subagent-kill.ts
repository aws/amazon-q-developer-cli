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
 * The pending approval is keyed by `toolCall.toolCallId`. We look up the
 * matching ToolUse message in the parent's `messages` list and compare
 * its `agentName` (the stage name attached when the tool call originated
 * inside a subagent session) against the killed stage's name.
 *
 * Returns false when:
 *   - No approval is pending.
 *   - The pending approval's tool isn't in `messages` (race — backend
 *     hasn't surfaced the ToolUse yet, or it was already finalized).
 *   - The pending approval is for a tool from the MAIN agent or a
 *     different subagent stage (we kill stage A, but stage B's approval
 *     shouldn't be touched).
 *
 * Returns true only when the killed stage owns the pending tool —
 * dropping it prevents the user from staring at an approval prompt for
 * a session that no longer exists. The session's process is gone the
 * moment `terminateSession` lands; any answer to the prompt would just
 * fail silently at the backend.
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
