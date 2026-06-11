/**
 * Parsing helpers for the KAS `_kiro/governance/state` notification
 * (kiro-agent PR #1145). The agent pushes the full resolved governance state
 * on session create/restore and per-turn identity change. The CLI consumes
 * the toggles it surfaces to the user.
 *
 * MCP governance is NOT parsed here: KAS still emits the dedicated
 * `_kiro/mcp/governance_disabled` notification, which the CLI already handles.
 * Reading mcpEnabled here too would double-process it.
 */

/** Reasons a feature is governance-disabled (mirrors the agent's enum). */
type GovernanceDisabledReason =
  | 'admin_disabled'
  | 'api_failure'
  | 'no_endpoint';

/**
 * Returns the web-tools-disabled signal when the notification indicates web
 * tools are governance-disabled, otherwise `undefined` (enabled / absent).
 *
 * `apiFailure` is `true` for the fail-closed paths (`api_failure`,
 * `no_endpoint`) and `false` for an explicit admin toggle — matching the
 * boolean the existing `handleWebToolsGovernanceDisabled` handler expects.
 */
export function webToolsGovernanceFromState(
  params: Record<string, unknown>
): { apiFailure: boolean } | undefined {
  const features = params.features as { webToolsEnabled?: boolean } | undefined;
  if (features?.webToolsEnabled !== false) return undefined;

  const reason = params.disabledReason as GovernanceDisabledReason | undefined;
  const apiFailure = reason === 'api_failure' || reason === 'no_endpoint';
  return { apiFailure };
}
