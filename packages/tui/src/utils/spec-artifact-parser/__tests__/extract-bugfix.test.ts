import { describe, expect, it } from 'bun:test';
import { extractBugfix } from '../extract-bugfix.js';

/** The shape the bugfix workflow requires, as it writes it. */
const DOC = `# Bugfix Requirements Document

## Introduction

The countdown keeps decrementing past zero and never fires the alert.
The root cause is in Timer.tick().

## Bug Analysis

### Current Behavior (Defect)

1.1 WHEN the countdown reaches zero THEN the system continues decrementing

1.2 WHEN seconds becomes negative THEN the system renders garbled output

### Expected Behavior (Correct)

2.1 WHEN the countdown reaches zero THEN the system SHALL stop decrementing

### Unchanged Behavior (Regression Prevention)

3.1 WHEN in Stopwatch mode THEN the system SHALL CONTINUE TO count up

3.2 WHEN Reset is pressed THEN the system SHALL CONTINUE TO stop the timer
`;

describe('extractBugfix', () => {
  it('reads the three behaviour sections and their clauses', () => {
    const { sections } = extractBugfix(DOC);

    expect(sections.map((s) => s.title)).toEqual([
      'Current Behavior (Defect)',
      'Expected Behavior (Correct)',
      'Unchanged Behavior (Regression Prevention)',
    ]);
    expect(sections.map((s) => s.clauses.length)).toEqual([2, 1, 2]);
  });

  it('keeps each clause number apart from its text', () => {
    const [current] = extractBugfix(DOC).sections;

    expect(current!.clauses[0]).toEqual({
      number: '1.1',
      text: 'WHEN the countdown reaches zero THEN the system continues decrementing',
    });
  });

  it('takes the overview from the introduction, first paragraph only', () => {
    const { overview } = extractBugfix(DOC);

    expect(overview).toBe(
      'The countdown keeps decrementing past zero and never fires the alert. The root cause is in Timer.tick().'
    );
  });

  it('gives each section a slice that stops at the next heading', () => {
    const [current, expected] = extractBugfix(DOC).sections;

    expect(current!.detailBody).toContain('1.2 WHEN seconds becomes negative');
    // The slice must not run into the section below it, or a comment left on one
    // section would quote lines belonging to another.
    expect(current!.detailBody).not.toContain('2.1');
    expect(expected!.detailBody).toContain('2.1');
  });

  it('skips a heading with no clauses rather than listing it empty', () => {
    const { sections } = extractBugfix(
      [
        '### Current Behavior (Defect)',
        '',
        'prose but no numbered clauses',
      ].join('\n')
    );

    expect(sections).toEqual([]);
  });

  it('returns nothing for an empty document', () => {
    expect(extractBugfix('')).toEqual({ overview: '', sections: [] });
  });
});
