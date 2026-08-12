import { describe, expect, it } from 'bun:test';
import {
  routeCheckpointAnswer,
  stagedCommentsOption,
} from '../checkpoint-answer.js';
import { anchorFor, type ReviewAction } from '../review-actions.js';

const LINES = [
  '# Requirements',
  '',
  '### Requirement 1: Mode Selection',
  '',
  '1. WHEN the page loads, THE Mode_Selector SHALL select Count_Up_Mode.',
];

const comment = (id: string, line: number, body: string): ReviewAction => ({
  kind: 'comment',
  id,
  anchor: anchorFor(LINES, { start: line, end: line }),
  body,
});

const reviewOf = (comments: ReviewAction[]) => ({
  document: 'requirements',
  comments,
});

const CONTINUE = 'Continue to design phase';

describe('stagedCommentsOption', () => {
  it('offers nothing without a checkpoint or without comments', () => {
    expect(stagedCommentsOption(null)).toBeNull();
    expect(stagedCommentsOption(reviewOf([]))).toBeNull();
  });

  it('counts the comments it would send', () => {
    expect(stagedCommentsOption(reviewOf([comment('a', 4, 'x')]))).toBe(
      'Send 1 comment and revise'
    );
    expect(
      stagedCommentsOption(
        reviewOf([comment('a', 4, 'x'), comment('b', 2, 'y')])
      )
    ).toBe('Send 2 comments and revise');
  });
});

describe('routeCheckpointAnswer', () => {
  it('passes any answer through when nothing is staged', () => {
    expect(routeCheckpointAnswer(null, CONTINUE)).toEqual({ kind: 'pass' });
    expect(routeCheckpointAnswer(reviewOf([]), CONTINUE)).toEqual({
      kind: 'pass',
    });
  });

  it('sends the composed revision request, not the option text', () => {
    const staged = [comment('a', 4, 'drop the second criterion')];
    const option = stagedCommentsOption(reviewOf(staged))!;

    const route = routeCheckpointAnswer(reviewOf(staged), option);

    expect(route.kind).toBe('send');
    if (route.kind !== 'send') return;
    // What the agent receives has to carry the comment and the anchor that
    // locates it — the display string alone would tell the agent nothing.
    expect(route.answerForAgent).toContain('drop the second criterion');
    expect(route.answerForAgent).toContain('requirements.md');
    expect(route.answerForAgent).toContain('quote="1. WHEN the page loads');
    expect(route.answerForAgent).toContain(
      'on="Requirement 1: Mode Selection"'
    );
    expect(route.answerForAgent).not.toBe(option);
  });

  it('refuses to advance while comments are staged, naming the way to send', () => {
    const staged = [comment('a', 4, 'x'), comment('b', 2, 'y')];

    const route = routeCheckpointAnswer(reviewOf(staged), CONTINUE);

    expect(route.kind).toBe('refuse');
    if (route.kind !== 'refuse') return;
    expect(route.message).toContain('2 comments staged');
    expect(route.message).toContain('Send 2 comments and revise');
    expect(route.message).toContain('review and remove');
  });

  it('refuses free text too, so a typed answer cannot drop comments either', () => {
    const route = routeCheckpointAnswer(
      reviewOf([comment('a', 4, 'x')]),
      'actually just tighten the intro'
    );

    expect(route.kind).toBe('refuse');
  });
});
