import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

/**
 * KAS-mode dispatch handler for `/memories`.
 *
 * Displays a panel with information about the memories feature — repo-scoped
 * knowledge extracted from past agent sessions (setup steps that worked, errors
 * encountered and how they were resolved, and repo-specific quirks).
 *
 * The feature is gated by the Feature::Memory rollout (traffic control).
 * Per-user opt-in via the onboarding service (getInstancePreference) is
 * not yet wired — KRS enforces per-call authorization as the final gate.
 */
export async function handleMemories(
  _cmd: KasCommand,
  _args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  ctx.setShowMemoriesPanel(true);
}
