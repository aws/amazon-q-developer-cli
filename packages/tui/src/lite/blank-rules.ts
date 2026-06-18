/**
 * Single source of truth for "should there be a blank line between two
 * adjacent rows in lite mode?" Three sites used to inline this rule:
 *
 *   - components/layout/lite/static-flush.ts → needsLeadingBlank
 *     (used by LiteLayout's <Static> append loop and LiteSubagentPanel)
 *   - lite/render.ts → previewNeedsLeadingBlank
 *     (used by /verbosity preview panes)
 *   - components/layout/lite/LiteLiveRegion.tsx → needsLeadingSeparator
 *     (used to gap the live region above the last static row)
 *
 * They were required to agree by hand-sync comments. Now they all delegate
 * to {@link needsLeadingBlankByRole} so the rule lives in exactly one place.
 *
 * The helper is role-keyed (string only, no message body) so it's usable
 * from both sides of the layout/render boundary without crossing dependency
 * direction. Each call site converts its own message shape to a role string.
 *
 * Lite-mode rules (hard-coded, no configurability):
 *   1. Blank BEFORE every user message.
 *   2. Blank AFTER every user message.
 *   3. NO blank between consecutive tool calls in the same turn.
 *   4. Blank between the LAST tool call and the agent's text response.
 *   5. Blank around system rows when adjacent to non-system rows.
 *
 * Adding a new role means adding a clause here, not editing renderers.
 */
export type BlankRuleRole = 'user' | 'model' | 'tool_use' | 'system';

export function needsLeadingBlankByRole(
  prev: BlankRuleRole,
  next: BlankRuleRole
): boolean {
  // Any message after the user gets a blank (rule 2).
  if (prev === 'user') return true;
  // Conversational turn boundary — model reply followed by next user input
  // (rule 1, model→user). The input divider draws ABOVE the prompt area,
  // not above the last scrollback row, so without an explicit blank the
  // new user line glues to the previous reply.
  if (prev === 'model' && next === 'user') return true;
  // Model ↔ Tool boundaries on both sides (rule 4 + its inverse).
  if (prev === 'model' && next === 'tool_use') return true;
  if (prev === 'tool_use' && next === 'model') return true;
  // Trailing-tool turn followed by next user input (rule 1, tool→user).
  if (prev === 'tool_use' && next === 'user') return true;
  // System lines stand alone — blank on both sides whenever they neighbor
  // any conversational/tool row. Adjacent System↔System stays compact.
  if (prev === 'system' && next !== 'system') return true;
  if (next === 'system' && prev !== 'system') return true;
  // Tool ↔ Tool, Model ↔ Model: compact (rule 3 + coalesced model spans).
  return false;
}
