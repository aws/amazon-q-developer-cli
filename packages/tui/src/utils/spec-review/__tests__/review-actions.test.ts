import { describe, it, expect } from 'bun:test';
import {
  anchorFor,
  composeRevisionRequest,
  summarizeRevision,
  findEnclosingHeading,
  navigableStops,
  nextHeadingLine,
  type ReviewAction,
} from '../review-actions';

const DOC = `# Requirements

## Introduction

A browser clock.

### Requirement 1: Mode Selection

**User Story:** As a visitor, I want to choose a mode.

#### Acceptance Criteria

1. WHEN the page loads, THE Mode_Selector SHALL select Count_Up_Mode.

### Requirement 2: Count-Up Timing

1. WHEN the user starts, THE Stopwatch SHALL enter Running_State.`.split('\n');

/** Index of the first line starting with `prefix`. */
function lineOf(prefix: string): number {
  const index = DOC.findIndex((line) => line.startsWith(prefix));
  if (index < 0) throw new Error(`no line starts with ${prefix}`);
  return index;
}

function commentOn(lineIndex: number, body: string): ReviewAction {
  return {
    kind: 'comment',
    id: 'c1',
    anchor: anchorFor(DOC, { start: lineIndex, end: lineIndex }),
    body,
  };
}

describe('findEnclosingHeading', () => {
  it('names the nearest heading above the line', () => {
    const line = lineOf('1. WHEN the user starts');
    expect(findEnclosingHeading(DOC, line)).toBe(
      'Requirement 2: Count-Up Timing'
    );
  });

  it('reaches past intervening prose to the closest heading of any level', () => {
    const line = lineOf('1. WHEN the page loads');
    // #### Acceptance Criteria sits between the line and the requirement.
    expect(findEnclosingHeading(DOC, line)).toBe('Acceptance Criteria');
  });

  it('returns null above the first heading', () => {
    expect(findEnclosingHeading(['plain text', 'more'], 1)).toBeNull();
  });

  it('tolerates a line index past the end', () => {
    expect(findEnclosingHeading(DOC, 999)).toBe(
      'Requirement 2: Count-Up Timing'
    );
  });
});

describe('anchorFor', () => {
  it('quotes the line trimmed and records where it was', () => {
    const line = lineOf('**User Story:**');
    const anchor = anchorFor(DOC, { start: line, end: line });
    expect(anchor.range.start).toBe(line);
    expect(anchor.heading).toBe('Requirement 1: Mode Selection');
    expect(anchor.snippet).toBe(
      '**User Story:** As a visitor, I want to choose a mode.'
    );
  });
});

describe('composeRevisionRequest', () => {
  it('sends each comment with the section and line it annotates', () => {
    const text = composeRevisionRequest('requirements.md', [
      commentOn(lineOf('1. WHEN the user starts'), 'drop laps'),
    ]);

    expect(text).toContain('on="Requirement 2: Count-Up Timing"');
    expect(text).toContain('quote="1. WHEN the user starts');
    expect(text).toContain('drop laps');
    expect(text).toContain('Revise requirements.md');
  });

  it('orders comments by their position in the document', () => {
    const later = lineOf('### Requirement 2');
    const earlier = lineOf('### Requirement 1');
    const text = composeRevisionRequest('requirements.md', [
      commentOn(later, 'second note'),
      commentOn(earlier, 'first note'),
    ]);

    expect(text.indexOf('first note')).toBeLessThan(
      text.indexOf('second note')
    );
  });

  it('never starts with a slash, which would read as a command', () => {
    const text = composeRevisionRequest('requirements.md', [
      commentOn(1, '/tmp/foo is the wrong path'),
    ]);
    expect(text.startsWith('/')).toBe(false);
  });

  it('escapes markup in the quoted line and heading', () => {
    const lines = ['## <Section> "A"', 'value < 10 && flag == "on"'];
    const text = composeRevisionRequest('design.md', [
      {
        kind: 'comment',
        id: 'c2',
        anchor: anchorFor(lines, { start: 1, end: 1 }),
        body: 'tighten this',
      },
    ]);

    expect(text).toContain('on="&lt;Section&gt; &quot;A&quot;"');
    expect(text).toContain('&amp;&amp;');
    expect(text).not.toContain('quote="value < 10');
  });

  it('omits the anchor attributes when there is nothing to anchor to', () => {
    const text = composeRevisionRequest('tasks.md', [
      {
        kind: 'comment',
        id: 'c3',
        anchor: anchorFor(['', ''], { start: 0, end: 0 }),
        body: 'add a task',
      },
    ]);

    expect(text).toContain('<comment>');
    expect(text).toContain('add a task');
  });
});

describe('navigableStops', () => {
  const anchor = (lineIndex: number) => ({
    range: { start: lineIndex, end: lineIndex },
    heading: null,
    snippet: '',
  });

  it('places each comment right after the line it annotates', () => {
    const stops = navigableStops(3, [
      { kind: 'comment', id: 'b', anchor: anchor(1), body: 'second' },
      { kind: 'comment', id: 'a', anchor: anchor(0), body: 'first' },
    ]);
    expect(stops).toEqual([
      { lineIndex: 0, commentId: null },
      { lineIndex: 0, commentId: 'a' },
      { lineIndex: 1, commentId: null },
      { lineIndex: 1, commentId: 'b' },
      { lineIndex: 2, commentId: null },
    ]);
  });

  it('keeps several comments on one line in the order they were made', () => {
    const stops = navigableStops(1, [
      { kind: 'comment', id: 'a', anchor: anchor(0), body: 'one' },
      { kind: 'comment', id: 'b', anchor: anchor(0), body: 'two' },
    ]);
    expect(stops.map((stop) => stop.commentId)).toEqual([null, 'a', 'b']);
  });
});

describe('nextHeadingLine', () => {
  const LINES = [
    '# Requirements Document',
    'intro prose',
    '### Requirement 1: Mode Selection',
    '1. criterion',
    '### Requirement 2: Timing',
    '2. criterion',
  ];

  it('skips the lines between headings going down', () => {
    expect(nextHeadingLine(LINES, 0, 1)).toBe(2);
    expect(nextHeadingLine(LINES, 2, 1)).toBe(4);
  });

  it('finds the heading above going up', () => {
    expect(nextHeadingLine(LINES, 5, -1)).toBe(4);
    expect(nextHeadingLine(LINES, 4, -1)).toBe(2);
  });

  it('reports nothing past the last or first heading', () => {
    expect(nextHeadingLine(LINES, 4, 1)).toBeNull();
    expect(nextHeadingLine(LINES, 0, -1)).toBeNull();
  });

  it('ignores a hash that is not a heading', () => {
    expect(nextHeadingLine(['a', '#no-space', '## real'], 0, 1)).toBe(2);
  });
});

describe('summarizeRevision', () => {
  it('shows the comments as they were typed, under their section', () => {
    const lines = [
      '## Glossary',
      '- **Duration**: A user-specified amount of time',
      '## Requirements',
      '1. WHEN the timer starts',
    ];
    const text = summarizeRevision('requirements.md', [
      {
        kind: 'comment',
        id: 'b',
        anchor: anchorFor(lines, { start: 3, end: 3 }),
        body: 'tighten this',
      },
      {
        kind: 'comment',
        id: 'a',
        anchor: anchorFor(lines, { start: 1, end: 1 }),
        body: 'drop the hour field',
      },
    ]);

    expect(text).toBe(
      [
        'Reviewed requirements.md and left 2 comments:',
        '- drop the hour field (Glossary)',
        '- tighten this (Requirements)',
      ].join('\n')
    );
  });

  it('carries none of the request written for the agent', () => {
    const text = summarizeRevision('design.md', [
      {
        kind: 'comment',
        id: 'a',
        anchor: anchorFor(['## Overview', 'text'], { start: 1, end: 1 }),
        body: 'say why',
      },
    ]);

    // The tagged form is unreadable in a transcript; that is the whole point of
    // showing this instead.
    expect(text).not.toContain('<comment');
    expect(text).not.toContain('quote=');
    expect(text).toContain('say why');
  });
});
