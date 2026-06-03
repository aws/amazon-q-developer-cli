/**
 * Fixture-specific assertions for the V2 -> KAS converted
 * `basic_fs_tools` session. Lives next to `basic-fs-tools.test.ts`,
 * which is the canonical test that owns the schema. Other tests that
 * reuse this fixture (e.g. `--resume`, `/chat`) call this helper
 * instead of duplicating per-tool checks.
 *
 * Generic invariants (paired tool_call/tool_result, payload type
 * allowlist, etc.) live in `assertions.ts` and apply to every
 * fixture.
 */

import { expect } from 'bun:test';
import type { PersistedMessage } from '@kiro/acp-type-covenant';
import { BASIC_FS_TOOLS } from './v2_fixtures';

/**
 * Asserts:
 * - 14 total messages: 1 user, 6 tool_call, 6 tool_result, 1 final
 *   assistant `Say`. V2 AssistantMessage entries that carry an empty
 *   text block + a ToolUse drop the empty text on conversion, so the
 *   only assistant payload is the summary.
 * - The user prompt's id, content prefix, and `source` are preserved.
 * - Each `tool_call` matches the fixture's pinned id + name in order,
 *   has `status: 'completed'`, and carries non-array object `args`.
 * - The first tool_call's `args` are passed through verbatim from the
 *   V2 source (KAS does not rewrite tool inputs on conversion).
 * - Each `tool_result` matches its `tool_call`'s id and is `success`.
 * - The single assistant payload is the final `Say` summary.
 */
export function assertBasicFsToolsConverted(
  messages: PersistedMessage[]
): void {
  expect(messages).toHaveLength(14);

  const users = messages.filter((m) => m.payload.type === 'user');
  expect(users).toHaveLength(1);
  expect(users[0]!.id).toBe(BASIC_FS_TOOLS.userPromptId);
  if (users[0]!.payload.type === 'user') {
    expect(users[0]!.payload.content).toContain('Do these six steps in order');
    expect(users[0]!.payload.source).toBe('chat');
  }

  const toolCalls = messages.filter((m) => m.payload.type === 'tool_call');
  expect(toolCalls).toHaveLength(BASIC_FS_TOOLS.toolUses.length);
  for (let i = 0; i < BASIC_FS_TOOLS.toolUses.length; i++) {
    const call = toolCalls[i]!;
    expect(call.payload.type).toBe('tool_call');
    if (call.payload.type !== 'tool_call') continue;
    expect(call.payload.toolCallId).toBe(BASIC_FS_TOOLS.toolUses[i]!.id);
    expect(call.payload.toolName).toBe(BASIC_FS_TOOLS.toolUses[i]!.name);
    expect(call.payload.status).toBe('completed');
    // KAS rejects non-object args; the converter must surface non-object
    // V2 inputs as a hard error, never as a wrapped value.
    expect(typeof call.payload.args).toBe('object');
    expect(Array.isArray(call.payload.args)).toBe(false);
  }

  const firstCall = toolCalls[0]!;
  if (firstCall.payload.type === 'tool_call') {
    // Tool args reference the cwd recorded on the fixture and pass
    // through to KAS unchanged regardless of the runtime bucket.
    expect(firstCall.payload.args).toEqual({
      command: 'create',
      path: `${BASIC_FS_TOOLS.capturedToolInputCwd}/notes.md`,
      content: 'kiro fixture notes\n',
    });
  }

  const toolResults = messages.filter((m) => m.payload.type === 'tool_result');
  expect(toolResults).toHaveLength(BASIC_FS_TOOLS.toolUses.length);
  for (let i = 0; i < BASIC_FS_TOOLS.toolUses.length; i++) {
    const result = toolResults[i]!;
    expect(result.payload.type).toBe('tool_result');
    if (result.payload.type !== 'tool_result') continue;
    expect(result.payload.toolCallId).toBe(BASIC_FS_TOOLS.toolUses[i]!.id);
    expect(result.payload.success).toBe(true);
  }

  const assistants = messages.filter((m) => m.payload.type === 'assistant');
  expect(assistants).toHaveLength(1);
  if (assistants[0]!.payload.type === 'assistant') {
    expect(assistants[0]!.payload.operationType).toBe('Say');
    expect(assistants[0]!.payload.content).toContain('All six steps done');
  }
}
