/**
 * Unit tests for `deriveStatusBilling`.
 *
 * The cases that matter are the ones where showing a number would be wrong: an
 * unlimited plan reports a sentinel limit, and a percentage of unlimited means
 * nothing, so both figures must stay null and the segments paint nothing rather
 * than a misleading `0%`.
 */
import { describe, it, expect } from 'bun:test';
import { deriveStatusBilling } from '../billing.js';
import type { UsageData } from '../../../../stores/app-store.js';

function usage(
  breakdowns: Array<Partial<UsageData['usageBreakdowns'][number]>>
): UsageData {
  return {
    planName: 'KIRO POWER',
    billingCycleReset: '2026-08-01',
    overagesEnabled: false,
    isEnterprise: false,
    bonusCredits: [],
    addOnCredits: [],
    overageCapable: true,
    usageBreakdowns: breakdowns.map((item) => ({
      displayName: 'Credits',
      used: 0,
      limit: 0,
      percentage: 0,
      currentOverages: 0,
      overageRate: 0,
      overageCharges: 0,
      currency: 'USD',
      hasLimit: true,
      ...item,
    })),
  };
}

describe('deriveStatusBilling', () => {
  it('reports both figures from the limited dimension', () => {
    expect(
      deriveStatusBilling(usage([{ used: 2040.28, limit: 10000 }]))
    ).toEqual({ usagePercent: 20, creditsRemaining: 7959.72 });
  });

  it('returns nothing when there is no payload yet', () => {
    expect(deriveStatusBilling(null)).toEqual({
      usagePercent: null,
      creditsRemaining: null,
    });
    expect(deriveStatusBilling(undefined)).toEqual({
      usagePercent: null,
      creditsRemaining: null,
    });
  });

  it('returns nothing on an unlimited plan rather than 0%', () => {
    expect(deriveStatusBilling(usage([{ hasLimit: false, limit: 0 }]))).toEqual(
      {
        usagePercent: null,
        creditsRemaining: null,
      }
    );
  });

  it('skips unlimited dimensions to find the limited one', () => {
    expect(
      deriveStatusBilling(
        usage([
          { displayName: 'Requests', hasLimit: false, limit: 0 },
          { displayName: 'Credits', used: 50, limit: 200 },
        ])
      )
    ).toEqual({ usagePercent: 25, creditsRemaining: 150 });
  });

  it('clamps an over-limit account instead of exceeding 100%', () => {
    expect(deriveStatusBilling(usage([{ used: 120, limit: 100 }]))).toEqual({
      usagePercent: 100,
      creditsRemaining: 0,
    });
  });

  it('returns nothing when the payload carries no dimensions', () => {
    expect(deriveStatusBilling(usage([]))).toEqual({
      usagePercent: null,
      creditsRemaining: null,
    });
  });
});
