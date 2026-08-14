/**
 * The single source of truth for whether workflows are on.
 *
 * Two independent conditions decide it, and conflating them is how the
 * surface ends up half-disabled — KAS told `enabled: false` while the TUI
 * still advertises `/goal` and starts the workflow extension:
 *
 *   - `available` — the rollout reaches this user. Gates only the
 *     `/settings → features` control, so an opted-out user can opt back in.
 *   - `enabled`  — available AND the user has opted in. Gates everything
 *     else: KAS serialization, command filtering, extension startup, and
 *     event handling.
 *
 * The opt-in is deliberate: being in the rollout cohort does not turn
 * workflows on, it only makes the toggle reachable.
 */

import { Feature, features } from '../features';
import { readBoolSetting } from './cli-settings';
import { Settings } from '../constants/settings';

export interface WorkflowsPolicy {
  /** Rollout reaches this user — the settings control is reachable. */
  available: boolean;
  /** Rollout reaches this user AND they opted in — the feature is live. */
  enabled: boolean;
}

export function resolveWorkflowsPolicy(): WorkflowsPolicy {
  const available = features.isEnabled(Feature.Workflows);
  return {
    available,
    enabled:
      available && readBoolSetting(Settings.CHAT_ENABLE_WORKFLOWS, false),
  };
}
