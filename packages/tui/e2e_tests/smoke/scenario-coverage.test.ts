import { describe, expect, it } from 'bun:test';

import { checkDrift, mergeLcov, summaryToFloor } from './scenario-coverage';

const lcovA = `TN:
SF:src/app.ts
DA:1,1
DA:2,0
DA:3,1
end_of_record
`;

const lcovB = `TN:
SF:src/app.ts
DA:1,0
DA:2,1
DA:4,1
end_of_record
SF:src/other.ts
DA:1,1
end_of_record
`;

describe('mergeLcov', () => {
  it('unions line hits across runs (a line hit in any run counts as hit)', () => {
    const summary = mergeLcov([lcovA, lcovB]);
    // app.ts lines: 1 (hit in A), 2 (hit in B), 3 (hit in A), 4 (hit in B) => 4/4
    expect(summary.files['src/app.ts']).toEqual({
      linesFound: 4,
      linesHit: 4,
      pct: 100,
    });
    expect(summary.files['src/other.ts']).toEqual({
      linesFound: 1,
      linesHit: 1,
      pct: 100,
    });
    expect(summary.total.linesFound).toBe(5);
    expect(summary.total.linesHit).toBe(5);
  });

  it('reports a partially-covered file', () => {
    const summary = mergeLcov([lcovA]);
    // app.ts: lines 1,3 hit; line 2 not => 2/3
    const app = summary.files['src/app.ts'];
    expect(app).toBeDefined();
    expect(app?.linesHit).toBe(2);
    expect(app?.linesFound).toBe(3);
    expect(app?.pct).toBeCloseTo(66.67, 1);
  });
});

describe('checkDrift', () => {
  const summary = mergeLcov([lcovA]); // app.ts 66.67%, total 66.67%

  it('passes when coverage meets the floor', () => {
    const floor = summaryToFloor(summary); // floors round down
    expect(checkDrift(summary, floor).ok).toBe(true);
  });

  it('flags a file that dropped below its floor', () => {
    const drift = checkDrift(summary, { 'src/app.ts': 90 });
    expect(drift.ok).toBe(false);
    expect(drift.regressions.join(' ')).toContain('src/app.ts');
  });

  it('flags a file that vanished from the run', () => {
    const drift = checkDrift(summary, { 'src/gone.ts': 10 });
    expect(drift.ok).toBe(false);
    expect(drift.regressions.join(' ')).toContain('src/gone.ts');
  });
});
