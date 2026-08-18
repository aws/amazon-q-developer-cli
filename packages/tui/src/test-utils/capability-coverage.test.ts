import { describe, expect, it } from 'bun:test';
import {
  calculateCapabilityCoverage,
  type CapabilityCoverageManifest,
  formatCapabilityCoverageMarkdown,
  parseCapabilityCoverageManifest,
  parsePassedTestEvidence,
  validateCapabilityEvidence,
} from './capability-coverage';

/** Pairs the evidence under test with an uncovered capability the gate must skip. */
function twoCapabilityManifest(evidence: string[]): CapabilityCoverageManifest {
  return parseCapabilityCoverageManifest({
    schemaVersion: 1,
    suite: 'Example',
    description: 'Example suite',
    denominator: 'Two declared capabilities.',
    capabilities: [
      {
        id: 'covered',
        area: 'wire',
        description: 'Covered behavior',
        evidence,
      },
      {
        id: 'missing',
        area: 'wire',
        description: 'Missing behavior',
        evidence: [],
      },
    ],
  });
}

// This module is the one file guaranteed to sit beside the test, so it stands in
// for an evidence file that exists.
const PRESENT_EVIDENCE = 'capability-coverage.ts#covers the behavior';

describe('capability coverage', () => {
  it('reports aggregate, per-area, and uncovered capability metrics', () => {
    const manifest = parseCapabilityCoverageManifest({
      schemaVersion: 1,
      suite: 'Example',
      description: 'Example suite',
      denominator: 'Two declared capabilities.',
      capabilities: [
        {
          id: 'covered',
          area: 'wire',
          description: 'Covered behavior',
          evidence: ['tests/covered.test.ts#covers the behavior'],
        },
        {
          id: 'missing',
          area: 'wire',
          description: 'Missing behavior',
          evidence: [],
        },
      ],
    });

    const report = calculateCapabilityCoverage(manifest);

    expect(report).toMatchObject({
      covered: 1,
      total: 2,
      percent: 50,
      areas: [{ area: 'wire', covered: 1, total: 2, percent: 50 }],
      uncovered: [{ id: 'missing' }],
    });
    expect(formatCapabilityCoverageMarkdown(report)).toContain(
      '1/2 capabilities (50.0%)'
    );
    expect(formatCapabilityCoverageMarkdown(report)).toContain(
      '`missing`: Missing behavior'
    );
  });

  it('rejects duplicate capability ids', () => {
    expect(() =>
      parseCapabilityCoverageManifest({
        schemaVersion: 1,
        suite: 'Example',
        description: 'Example suite',
        denominator: 'Duplicate ids.',
        capabilities: [
          {
            id: 'duplicate',
            area: 'wire',
            description: 'First',
            evidence: [],
          },
          {
            id: 'duplicate',
            area: 'ui',
            description: 'Second',
            evidence: [],
          },
        ],
      })
    ).toThrow('Duplicate capability id: duplicate');
  });

  it('reads only passed testcases from Bun JUnit output', () => {
    const passed = parsePassedTestEvidence(`
      <testsuites>
        <testsuite file="tests/example.test.ts">
          <testcase file="tests/example.test.ts" name="passes" />
          <testcase file="tests/example.test.ts" name="fails">
            <failure message="no" />
          </testcase>
          <testcase file="tests/example.test.ts" name="skips">
            <skipped />
          </testcase>
          <testcase file="tests\\windows.test.ts" name="normalizes paths" />
        </testsuite>
      </testsuites>
    `);

    expect([...passed]).toEqual([
      'tests/example.test.ts#passes',
      'tests/windows.test.ts#normalizes paths',
    ]);
  });

  it('accepts evidence that exists on disk and passed', async () => {
    await expect(
      validateCapabilityEvidence(
        twoCapabilityManifest([PRESENT_EVIDENCE]),
        import.meta.dir,
        new Set([PRESENT_EVIDENCE])
      )
    ).resolves.toBeUndefined();
  });

  it('rejects evidence whose test is absent from the passed set', async () => {
    await expect(
      validateCapabilityEvidence(
        twoCapabilityManifest([PRESENT_EVIDENCE]),
        import.meta.dir,
        new Set<string>()
      )
    ).rejects.toThrow(/did not pass in the test report/);
  });

  it('rejects a missing evidence file', async () => {
    // Kept in the passed set so only the file check can reject it.
    const absent = 'no-such-suite.test.ts#covers the behavior';
    await expect(
      validateCapabilityEvidence(
        twoCapabilityManifest([absent]),
        import.meta.dir,
        new Set([absent])
      )
    ).rejects.toThrow(/does not exist/);
  });

  it('rejects evidence that escapes the package root', async () => {
    const outside = '../outside.test.ts#covers the behavior';
    await expect(
      validateCapabilityEvidence(
        twoCapabilityManifest([outside]),
        import.meta.dir,
        new Set([outside])
      )
    ).rejects.toThrow(/escapes the package root/);
  });
});
