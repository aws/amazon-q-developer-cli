import type { KiroMeta } from '../types/agent-events.js';

/**
 * Normalized denial detail for a blocked tool call, rendered by
 * ToolDenialDetails. Parity with the IDE's SafetyDenialDetails /
 * PolicyDenialDetails cards: a source ("infrastructure safety" vs a permission
 * policy scope), the rule text, and the tool.
 */
export interface ToolDenial {
  /** Where the block came from, e.g. 'infrastructure safety' or 'workspace permission policy'. */
  readonly source: string;
  /** The violated rule(s), joined for display. */
  readonly rule: string;
  /** The gated tool name, when known. */
  readonly tool?: string;
}

/**
 * Derives a {@link ToolDenial} from a tool call's `_meta.kiro`, preferring the
 * infra-safety block (`safetyOverride`) then the permission-policy denial
 * (`policyDenial`). Returns undefined when neither is present. Mirrors how the
 * IDE routes to SafetyDenialDetails / PolicyDenialDetails off the same meta.
 */
export function deriveToolDenial(
  meta: KiroMeta | undefined
): ToolDenial | undefined {
  const safety = meta?.safetyOverride;
  // Gate on a non-allowed terminal `decision`, NOT on the mere presence of
  // safetyOverride: KAS stamps it on every step of the override lifecycle —
  // the pending card (no decision yet) and the terminal update alike. Without
  // this gate the "Blocked" card would show while the user is still deciding
  // and would persist even after they choose "Allow anyway" (decision:'allowed').
  // Only denied / cancelled / error are real blocks worth surfacing.
  if (safety && safety.decision && safety.decision !== 'allowed') {
    const rule =
      safety.blockedProperties && safety.blockedProperties.length > 0
        ? safety.blockedProperties.join(', ')
        : safety.reason;
    return { source: 'infrastructure safety', rule, tool: safety.toolName };
  }

  const policy = meta?.policyDenial;
  if (policy) {
    const patterns =
      policy.matchedRule.match && policy.matchedRule.match.length > 0
        ? policy.matchedRule.match.join(', ')
        : policy.resource;
    return {
      source: `${policy.scope} permission policy`,
      rule: `${policy.matchedRule.effect} ${patterns}`.trim(),
      tool: policy.capability,
    };
  }

  return undefined;
}
