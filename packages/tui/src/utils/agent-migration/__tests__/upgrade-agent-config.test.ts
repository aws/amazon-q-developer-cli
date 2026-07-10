/**
 * Data-driven tests for `upgradeAgentConfig` — the on-disk universal config a
 * customer gets from `/upgrade-agent` (V2 fields preserved, tools unioned,
 * permissions derived). `upgradeAgentFile` writes `result.config` verbatim, so
 * `result.config` here IS the on-disk content.
 *
 * Cases live in `upgrade-agent-config-cases/`:
 *   { description, input, expectedConfig, classification, warnings? }
 * (omit `warnings` ⇒ expect none)
 */

import { describe, test, expect } from 'bun:test';
import { readFileSync } from 'fs';
import { join } from 'path';

import { type AgentClassification, upgradeAgentConfig } from '../migrate.js';

const CASES_FILE = join(import.meta.dir, 'upgrade-agent-config-cases.json');

interface UpgradeCase {
  description?: string;
  input: Record<string, unknown>;
  expectedConfig: Record<string, unknown>;
  classification: AgentClassification;
  warnings?: string[];
}

function loadCases(): UpgradeCase[] {
  return JSON.parse(readFileSync(CASES_FILE, 'utf-8')) as UpgradeCase[];
}

const allCases = loadCases();

/**
 * Recursively sort arrays so comparison is order-independent: `tools` is a set,
 * and permission rules are evaluated by effect precedence (not array position),
 * so array order carries no meaning here — fixtures may list entries in any order.
 */
function sortDeep(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value
      .map(sortDeep)
      .sort((a, b) => (JSON.stringify(a) < JSON.stringify(b) ? -1 : 1));
  }
  if (value && typeof value === 'object') {
    const o = value as Record<string, unknown>;
    return Object.fromEntries(
      Object.keys(o)
        .sort()
        .map((k) => [k, sortDeep(o[k])])
    );
  }
  return value;
}

describe('upgradeAgentConfig — on-disk universal config (data-driven)', () => {
  test('discovers at least one case', () => {
    expect(allCases.length).toBeGreaterThan(0);
  });

  for (const testCase of allCases) {
    test(testCase.description ?? 'case', () => {
      const result = upgradeAgentConfig(testCase.input);

      // Order-independent: `tools`/rules are sets, so compare with arrays sorted.
      expect(sortDeep(result.config)).toEqual(
        sortDeep(testCase.expectedConfig)
      );
      expect(result.classification).toBe(testCase.classification);

      const expectedWarnings = (testCase.warnings ?? []).slice().sort();
      const actualWarnings: string[] = result.warnings
        .map((w) => w.kind)
        .sort();
      expect(actualWarnings).toEqual(expectedWarnings);
    });
  }
});
