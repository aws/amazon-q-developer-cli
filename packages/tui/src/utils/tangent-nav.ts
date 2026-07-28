/**
 * Pure navigation resolvers for /tangent. Extracted so the Explorer selection
 * decision is unit-testable without a React render tree — the picker resolves a
 * selected row to a concrete action rather than round-tripping through a title
 * or a bare command (which wrongly resolved the root row as "go back one level" and
 * self-selection as "create a new tangent").
 */

export type TangentSelection =
  | { action: 'noop' }
  | { action: 'switch'; sessionId: string };

/**
 * Resolve an Explorer row selection.
 *
 * A picker selects a session, so switch directly to the selected session's id —
 * for root, a sibling, or a descendant alike. Selecting the row you're already
 * on is a no-op (never a reload, never a create). Because we key off the exact
 * sessionId, this does not depend on parentSessionId being present.
 */
export function resolveTangentSelection(
  selectedSessionId: string,
  currentSessionId: string | undefined
): TangentSelection {
  if (!selectedSessionId) return { action: 'noop' };
  if (selectedSessionId === currentSessionId) return { action: 'noop' };
  return { action: 'switch', sessionId: selectedSessionId };
}
