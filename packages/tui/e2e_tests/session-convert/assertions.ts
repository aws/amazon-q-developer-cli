/**
 * Generic invariants every V2 -> KAS converted session must hold,
 * regardless of fixture. Per-fixture content assertions live in the
 * test files.
 *
 * Use this after loading a converted session via
 * `SessionPersistence.loadSession`. The invariants mirror what
 * `messagesToContextMessages` (KAS's context-build path) implicitly
 * relies on: paired tool_call/tool_result, no orphan turn markers,
 * `_meta.kiro.userMessageId` binding from non-user payloads to a
 * real user prompt id.
 */

import { expect } from 'bun:test';
import type { PersistedMessage } from '@kiro/acp-type-covenant';

/**
 * Payload types the V2 -> KAS converter emits. Anything else on
 * disk after conversion is unexpected.
 */
const ALLOWED_PAYLOAD_TYPES = new Set([
  'user',
  'assistant',
  'tool_call',
  'tool_result',
  'tombstone',
]);

/**
 * Shared invariants for V2 -> KAS converted sessions.
 *
 * Asserts:
 * - At least one message and at least one `user` message.
 * - Every payload type is in `ALLOWED_PAYLOAD_TYPES`. The converter
 *   emits user / assistant / tool_call / tool_result, plus a
 *   `tombstone` for a Compaction. Turn / sub_agent / mode_change
 *   markers are not emitted and must not appear on disk.
 * - For every `tool_call`, a `tool_result` with the same
 *   `toolCallId` appears later in the array. KAS's load-time
 *   normalizer would otherwise inject a synthetic failure result
 *   for the orphan; the converter emits both halves so that path
 *   is never triggered for V2-sourced sessions.
 * - Every `assistant` and `tool_call` payload's
 *   `_meta.kiro.userMessageId` resolves to a real `user` payload
 *   in the array.
 */
export function assertConvertedSession(messages: PersistedMessage[]): void {
  expect(messages.length).toBeGreaterThan(0);

  const userIds = new Set<string>();
  const toolCallOrder = new Map<string, number>();
  const toolResultOrder = new Map<string, number>();

  for (let i = 0; i < messages.length; i++) {
    const m = messages[i]!;
    expect(ALLOWED_PAYLOAD_TYPES.has(m.payload.type)).toBe(true);

    if (m.payload.type === 'user') {
      userIds.add(m.id);
    } else if (m.payload.type === 'tool_call') {
      toolCallOrder.set(m.payload.toolCallId, i);
    } else if (m.payload.type === 'tool_result') {
      toolResultOrder.set(m.payload.toolCallId, i);
    }
  }

  expect(userIds.size).toBeGreaterThan(0);

  // Every tool_call has a tool_result that comes after it.
  for (const [toolCallId, callIdx] of toolCallOrder) {
    const resultIdx = toolResultOrder.get(toolCallId);
    expect(resultIdx).toBeDefined();
    expect(resultIdx!).toBeGreaterThan(callIdx);
  }

  // userMessageId on assistant + tool_call payloads, when present,
  // binds to a real user message id. tool_result payloads bind via
  // toolCallId, not userMessageId, so they're skipped here. A Summary
  // assistant (compaction artifact) and a retained snapshot turn with
  // no preceding text user both legitimately omit the field; the
  // invariant is only that a present binding resolves.
  for (const m of messages) {
    if (m.payload.type !== 'assistant' && m.payload.type !== 'tool_call') {
      continue;
    }
    const userMessageId = m.payload._meta?.kiro?.userMessageId;
    if (userMessageId === undefined) {
      continue;
    }
    expect(userIds.has(userMessageId)).toBe(true);
  }
}

/** Every message id in a converted session is unique. */
export function assertUniqueMessageIds(messages: PersistedMessage[]): void {
  const ids = messages.map((m) => m.id);
  expect(new Set(ids).size).toBe(ids.length);
}

/**
 * Asserts every top-level V2 `Prompt` in `messagesJsonl` maps to
 * exactly one KAS `user` payload carrying the same id. The V2 prompt
 * id is preserved verbatim on conversion, so this is a clean 1:1 key
 * between the two formats. Snapshot users (re-emitted under a
 * compaction namespace) are not top-level V2 prompts and are excluded
 * by matching exact ids.
 */
export function assertPromptIdsMapToUsers(
  messagesJsonl: string,
  messages: PersistedMessage[]
): void {
  const userIdCounts = new Map<string, number>();
  for (const m of messages) {
    if (m.payload.type === 'user') {
      userIdCounts.set(m.id, (userIdCounts.get(m.id) ?? 0) + 1);
    }
  }
  for (const line of messagesJsonl.trim().split('\n')) {
    const entry = JSON.parse(line);
    if (entry.kind !== 'Prompt') continue;
    const promptId = entry.data.message_id as string;
    expect(userIdCounts.get(promptId)).toBe(1);
  }
}
