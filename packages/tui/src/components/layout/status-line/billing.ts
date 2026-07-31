/**
 * One payload feeds both figures: the account reports a single credits dimension,
 * so usage is `used/limit` and credits remaining is `limit - used` of that pair.
 */
import type { UsageData } from '../../../stores/app-store.js';

export interface StatusBilling {
  usagePercent: number | null;
  creditsRemaining: number | null;
}

export const EMPTY_STATUS_BILLING: StatusBilling = {
  usagePercent: null,
  creditsRemaining: null,
};

/**
 * Picks the dimension that has a limit. An unlimited plan reports a sentinel, and
 * a percentage of unlimited is meaningless, so both figures stay null and the
 * segments render nothing rather than `0%`.
 */
export function deriveStatusBilling(
  data: UsageData | null | undefined
): StatusBilling {
  if (!data) return EMPTY_STATUS_BILLING;
  const limited = data.usageBreakdowns.find(
    (item) => item.hasLimit && item.limit > 0
  );
  if (!limited) return EMPTY_STATUS_BILLING;

  const used = Math.max(0, limited.used);
  const remaining = Math.max(0, limited.limit - used);
  return {
    usagePercent: Math.min(100, Math.round((used / limited.limit) * 100)),
    creditsRemaining: remaining,
  };
}
