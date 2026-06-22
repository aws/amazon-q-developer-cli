export type BlankRuleRole = 'user' | 'model' | 'tool_use' | 'system';

export function needsLeadingBlankByRole(
  prev: BlankRuleRole,
  next: BlankRuleRole
): boolean {
  if (prev === 'user') return true;
  // model→user: the input divider draws above the prompt, not the last
  // scrollback row, so without this the new user line glues to the reply.
  if (prev === 'model' && next === 'user') return true;
  if (prev === 'model' && next === 'tool_use') return true;
  if (prev === 'tool_use' && next === 'model') return true;
  if (prev === 'tool_use' && next === 'user') return true;
  // System lines stand alone, except adjacent System↔System stays compact.
  if (prev === 'system' && next !== 'system') return true;
  if (next === 'system' && prev !== 'system') return true;
  return false;
}
