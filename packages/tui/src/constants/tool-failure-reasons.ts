/**
 * Canonical tool-failure reason strings the V2 (chat-cli-v2) Rust backend
 * stamps into a failed tool call's result text.
 *
 * ACP's wire-level `ToolCallStatus` only has `Completed` / `Failed`, so the
 * backend collapses user-cancel and user-deny into the same `Failed` shape and
 * tunnels the distinction through the failure *text*. The TUI recovers the
 * real outcome by matching that text against the strings below — see
 * `isUserCancelledReason` / `isUserDeniedReason`.
 *
 * These mirror the literals emitted in `crates/agent/src/agent/mod.rs`,
 * `crates/agent/src/agent/protocol.rs`, and
 * `crates/chat-cli-v2/src/agent/acp/acp_agent.rs`. They are V2-specific: KAS
 * does not stamp these reasons, so the predicates simply return false for it.
 * The proper long-term fix is a typed reason on the ACP `meta` envelope (see
 * `ToolCallFailureReason` on the Rust side); these constants localize the
 * string coupling to one place until that wire change lands.
 */

/** Emitted when the user interrupts (Esc) a running tool. */
export const TOOL_CANCELLED_BY_USER = 'Tool use was cancelled by the user';

/** Substrings present in the reason text when the user denied an approval. */
const DENIED_BY_USER_MARKERS = [
  'denied by the user',
  'rejected because the arguments supplied are forbidden',
];

/** Whether a V2 failure-reason string means "user interrupted this tool". */
export function isUserCancelledReason(text: unknown): boolean {
  return text === TOOL_CANCELLED_BY_USER;
}

/** Whether a V2 failure-reason string means "user denied this tool's approval". */
export function isUserDeniedReason(text: unknown): boolean {
  if (typeof text !== 'string') return false;
  const lower = text.toLowerCase();
  return DENIED_BY_USER_MARKERS.some((marker) => lower.includes(marker));
}
