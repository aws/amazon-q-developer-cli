import { describe, test, expect } from 'bun:test';
import { deriveToolDenial } from './tool-denial';
import type { KiroMeta } from '../types/agent-events';

describe('deriveToolDenial', () => {
  test('returns undefined when neither denial is present', () => {
    expect(deriveToolDenial(undefined)).toBeUndefined();
    expect(deriveToolDenial({} as KiroMeta)).toBeUndefined();
    expect(
      deriveToolDenial({ toolId: 'str_replace' } as KiroMeta)
    ).toBeUndefined();
  });

  test('maps a denied infra-safety block (safetyOverride) to source/rule/tool', () => {
    const out = deriveToolDenial({
      safetyOverride: {
        kind: 'infra-safety',
        toolName: 'str_replace',
        reason: 'Tool call violates safety properties',
        blockedProperties: [
          'Never delete the s3bucket',
          'Do not delete S3 buckets',
        ],
        decision: 'denied',
      },
    } as KiroMeta);
    expect(out).toEqual({
      source: 'infrastructure safety',
      rule: 'Never delete the s3bucket, Do not delete S3 buckets',
      tool: 'str_replace',
    });
  });

  test('falls back to reason when safetyOverride has no blockedProperties', () => {
    const out = deriveToolDenial({
      safetyOverride: {
        toolName: 'fs_write',
        reason: 'Could not prove safe',
        blockedProperties: [],
        decision: 'denied',
      },
    } as KiroMeta);
    expect(out).toEqual({
      source: 'infrastructure safety',
      rule: 'Could not prove safe',
      tool: 'fs_write',
    });
  });

  // ── Override lifecycle gating (regression for PR #3709 review) ──
  // KAS stamps safetyOverride on every step of the lifecycle: the pending card
  // (no decision) and the terminal update (decision set). The card must only
  // surface on a non-allowed terminal decision.

  test('returns undefined for a pending safetyOverride (no decision yet)', () => {
    const out = deriveToolDenial({
      safetyOverride: {
        kind: 'infra-safety',
        toolName: 'str_replace',
        reason: 'Tool call violates safety properties',
        blockedProperties: ['S rule'],
      },
    } as KiroMeta);
    expect(out).toBeUndefined();
  });

  test('returns undefined when the user allowed the override (decision=allowed)', () => {
    const out = deriveToolDenial({
      safetyOverride: {
        kind: 'infra-safety',
        toolName: 'str_replace',
        reason: 'r',
        blockedProperties: ['S rule'],
        decision: 'allowed',
      },
    } as KiroMeta);
    expect(out).toBeUndefined();
  });

  test.each(['denied', 'cancelled', 'error'] as const)(
    'surfaces the card for a non-allowed terminal decision (%s)',
    (decision) => {
      const out = deriveToolDenial({
        safetyOverride: {
          kind: 'infra-safety',
          toolName: 'str_replace',
          reason: 'r',
          blockedProperties: ['S rule'],
          decision,
        },
      } as KiroMeta);
      expect(out).toEqual({
        source: 'infrastructure safety',
        rule: 'S rule',
        tool: 'str_replace',
      });
    }
  );

  test('maps a permission-policy denial to source/rule/tool', () => {
    const out = deriveToolDenial({
      policyDenial: {
        capability: 'fs_write',
        resource: '/Users/x/.kiro/settings/mcp.json',
        effect: 'deny',
        scope: 'workspace',
        source: '/Users/x/.kiro/permissions.yaml',
        matchedRule: {
          capability: 'fs_write',
          effect: 'deny',
          match: ['~/.kiro/settings/'],
        },
      },
    } as KiroMeta);
    expect(out).toEqual({
      source: 'workspace permission policy',
      rule: 'deny ~/.kiro/settings/',
      tool: 'fs_write',
    });
  });

  test('policy denial falls back to resource when the rule has no match patterns', () => {
    const out = deriveToolDenial({
      policyDenial: {
        capability: 'shell',
        resource: 'rm -rf /',
        effect: 'deny',
        scope: 'user',
        source: '/p',
        matchedRule: { capability: 'shell', effect: 'deny' },
      },
    } as KiroMeta);
    expect(out).toEqual({
      source: 'user permission policy',
      rule: 'deny rm -rf /',
      tool: 'shell',
    });
  });

  test('prefers safetyOverride when both are present', () => {
    const out = deriveToolDenial({
      safetyOverride: {
        toolName: 'str_replace',
        reason: 'r',
        blockedProperties: ['S rule'],
        decision: 'denied',
      },
      policyDenial: {
        capability: 'fs_write',
        resource: 'x',
        effect: 'deny',
        scope: 'workspace',
        source: '/p',
        matchedRule: { capability: 'fs_write', effect: 'deny' },
      },
    } as KiroMeta);
    expect(out?.source).toBe('infrastructure safety');
    expect(out?.rule).toBe('S rule');
  });
});
