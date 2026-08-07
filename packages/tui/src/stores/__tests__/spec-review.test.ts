/**
 * Store-level tests for the spec review surface: moving between document lines
 * and the comments attached to them, staging, editing and dropping a single
 * comment, and the lifecycle that ties staged comments to their checkpoint.
 */
import { describe, it, expect, mock } from 'bun:test';
import { createAppStore } from '../app-store';

function makeStore() {
  const kiro = {
    setConfigOption: mock(() => Promise.resolve()),
    isCloudSessionActive: () => false,
    sendChatSlashCommandTelemetry: mock(),
    close: mock(),
  } as any;
  const store = createAppStore({ kiro });
  store.setState({ isInitialized: true, sessionId: 'test-session' });
  return store;
}

/** The nested surface and comments, so tests read them the way the app does. */
const review = (store: ReturnType<typeof makeStore>) =>
  store.getState().specPhaseCheckpoint?.review ?? null;
const comments = (store: ReturnType<typeof makeStore>) =>
  store.getState().specPhaseCheckpoint?.comments ?? [];

const LINES = [
  '# Requirements',
  '',
  '### Requirement 1: Mode Selection',
  '',
  '1. WHEN the page loads, THE Mode_Selector SHALL select Count_Up_Mode.',
  '2. WHEN the user switches, THE Web_Clock SHALL reset.',
];

function openReview(store: ReturnType<typeof makeStore>, lineIndex = 0) {
  store.setState({
    specPhaseCheckpoint: {
      featureName: 'web-clock',
      phase: 'requirements',
      artifactPath: '/w/.kiro/specs/web-clock/requirements.md',
      comments: store.getState().specPhaseCheckpoint?.comments ?? [],
      review: {
        lines: LINES,
        cursor: { lineIndex, commentId: null },
        composing: null,
        error: null,
      },
    },
  });
}

/** Stage a comment the way the surface does: start, then commit. */
function comment(store: ReturnType<typeof makeStore>, body: string) {
  store.getState().startSpecReviewComment();
  store.getState().commitSpecReviewComment(body);
}

describe('spec review surface', () => {
  it('stages a comment against the line under the cursor', () => {
    const store = makeStore();
    openReview(store, 4);

    comment(store, '  drop this criterion  ');

    const [action] = comments(store);
    expect(action?.body).toBe('drop this criterion');
    expect(action?.anchor.range.start).toBe(4);
    expect(action?.anchor.heading).toBe('Requirement 1: Mode Selection');
    expect(action?.anchor.snippet).toContain('Mode_Selector SHALL select');
    expect(review(store)?.composing).toBeNull();
  });

  it('leaves the cursor on the new comment so it can be revised at once', () => {
    const store = makeStore();
    openReview(store, 4);

    comment(store, 'first thought');

    const [action] = comments(store);
    expect(review(store)?.cursor.commentId).toBe(action!.id);
  });

  it('ignores an empty comment and leaves nothing staged', () => {
    const store = makeStore();
    openReview(store, 2);

    comment(store, '   ');

    expect(comments(store)).toHaveLength(0);
    expect(review(store)?.composing).toBeNull();
  });

  it('edits the comment under the cursor, prefilling what it says', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'original wording');

    store.getState().startSpecReviewComment();
    expect(review(store)?.composing?.draft).toBe('original wording');
    store.getState().commitSpecReviewComment('revised wording');

    const bodies = comments(store).map((a) => a.body);
    expect(bodies).toEqual(['revised wording']);
  });

  it('treats an emptied comment as a removed one', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'never mind this');

    store.getState().startSpecReviewComment();
    store.getState().commitSpecReviewComment('   ');

    expect(comments(store)).toHaveLength(0);
    expect(review(store)?.cursor.commentId).toBeNull();
  });

  it('removes only the comment under the cursor', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'first on line four');
    store.setState((state) => ({
      specPhaseCheckpoint: {
        ...state.specPhaseCheckpoint!,
        review: {
          ...state.specPhaseCheckpoint!.review!,
          cursor: { lineIndex: 4, commentId: null },
        },
      },
    }));
    comment(store, 'second on line four');
    const second = comments(store)[1]!;
    expect(review(store)?.cursor.commentId).toBe(second.id);

    store.getState().removeSpecReviewCommentAtCursor();

    const bodies = comments(store).map((a) => a.body);
    expect(bodies).toEqual(['first on line four']);
  });

  it('does nothing when asked to remove while on a document line', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'keep me');
    store.setState((state) => ({
      specPhaseCheckpoint: {
        ...state.specPhaseCheckpoint!,
        review: {
          ...state.specPhaseCheckpoint!.review!,
          cursor: { lineIndex: 4, commentId: null },
        },
      },
    }));

    store.getState().removeSpecReviewCommentAtCursor();

    expect(comments(store)).toHaveLength(1);
  });

  it('steps onto a comment before moving past its line', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'a note on four');
    const noteId = comments(store)[0]!.id;
    store.setState((state) => ({
      specPhaseCheckpoint: {
        ...state.specPhaseCheckpoint!,
        review: {
          ...state.specPhaseCheckpoint!.review!,
          cursor: { lineIndex: 4, commentId: null },
        },
      },
    }));

    store.getState().moveSpecReviewCursor(1);
    expect(review(store)?.cursor).toEqual({
      lineIndex: 4,
      commentId: noteId,
    });

    store.getState().moveSpecReviewCursor(1);
    expect(review(store)?.cursor).toEqual({
      lineIndex: 5,
      commentId: null,
    });
  });

  it('stops at the document edges', () => {
    const store = makeStore();
    openReview(store, 0);

    store.getState().moveSpecReviewCursor(-5);
    expect(review(store)?.cursor.lineIndex).toBe(0);

    store.getState().moveSpecReviewCursor(50);
    expect(review(store)?.cursor.lineIndex).toBe(LINES.length - 1);
  });

  it('jumps to the next heading rather than the next line', () => {
    const store = makeStore();
    openReview(store, 0);

    store.getState().moveSpecReviewCursorToSection(1);

    expect(review(store)?.cursor.lineIndex).toBe(2);
    expect(LINES[2]).toContain('Requirement 1');
  });

  it('lands on the document edge when there is no further heading', () => {
    const store = makeStore();
    openReview(store, 2);

    store.getState().moveSpecReviewCursorToSection(1);

    expect(review(store)?.cursor.lineIndex).toBe(LINES.length - 1);
  });

  it('jumps between staged comments, wherever the cursor starts', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'on the first criterion');
    openReview(store, 5);
    comment(store, 'on the second criterion');

    // From above every comment, forward reaches the first one.
    openReview(store, 0);
    store.getState().moveSpecReviewCursorToComment(1);
    expect(review(store)?.cursor.lineIndex).toBe(4);
    expect(review(store)?.cursor.commentId).toBeTruthy();

    // On a comment, forward reaches its neighbour.
    store.getState().moveSpecReviewCursorToComment(1);
    expect(review(store)?.cursor.lineIndex).toBe(5);

    // And stops at the last rather than wrapping to the first.
    store.getState().moveSpecReviewCursorToComment(1);
    expect(review(store)?.cursor.lineIndex).toBe(5);

    store.getState().moveSpecReviewCursorToComment(-1);
    expect(review(store)?.cursor.lineIndex).toBe(4);
  });

  it('does nothing when no comment is staged to jump to', () => {
    const store = makeStore();
    openReview(store, 2);

    store.getState().moveSpecReviewCursorToComment(1);

    expect(review(store)?.cursor.lineIndex).toBe(2);
    expect(review(store)?.cursor.commentId).toBeNull();
  });

  it('keeps staged comments when the surface closes', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'keep me');

    store.getState().closeSpecReview();

    expect(review(store)).toBeNull();
    expect(comments(store)).toHaveLength(1);
  });

  it('drops staged comments once the checkpoint question resolves', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'stale after answering');
    const question = {
      sessionId: 'test-session',
      toolCallId: 'q-1',
      question: 'Review the requirements, then:',
      options: [{ title: 'Continue to design phase' }],
      resolve: mock(),
    } as any;
    store.setState({ pendingQuestion: question, questionQueue: [question] });

    store.getState().respondToQuestion('Continue to design phase', question);

    expect(comments(store)).toHaveLength(0);
    expect(review(store)).toBeNull();
  });

  // Retiring a checkpoint has to take the surface and the comments with it:
  // comments outliving their checkpoint would be counted into the next one and
  // sent quoting a document the agent never wrote. They hang off the checkpoint
  // so that holds for every path that nulls it, including the turn-end expiry
  // these cases can't reach without a full stream harness.
  const pendingCheckpointQuestion = (store: ReturnType<typeof makeStore>) => {
    const question = {
      sessionId: 'test-session',
      toolCallId: 'q-1',
      question: 'Review the requirements, then:',
      options: [{ title: 'Continue to design phase' }],
      resolve: mock(),
    } as any;
    store.setState({ pendingQuestion: question, questionQueue: [question] });
    return question;
  };

  it('takes the surface and comments with it when the question is cancelled', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'unsent');
    pendingCheckpointQuestion(store);

    store.getState().cancelQuestion();

    expect(store.getState().specPhaseCheckpoint).toBeNull();
    expect(comments(store)).toHaveLength(0);
    expect(review(store)).toBeNull();
  });

  it('takes them with it when the owning session terminates', () => {
    const store = makeStore();
    openReview(store, 4);
    comment(store, 'unsent');
    pendingCheckpointQuestion(store);

    store.getState().cleanupTerminatedSession('test-session');

    expect(store.getState().specPhaseCheckpoint).toBeNull();
    expect(comments(store)).toHaveLength(0);
    expect(review(store)).toBeNull();
  });

  it('drops a document that finished loading after the checkpoint moved on', async () => {
    const store = makeStore();
    store.setState({
      specPhaseCheckpoint: {
        featureName: 'web-clock',
        phase: 'requirements',
        artifactPath: '/w/.kiro/specs/web-clock/requirements.md',
        comments: [],
        review: null,
      },
    });

    const opening = store.getState().openSpecReview();
    // The next phase lands while the read is still in flight.
    store.setState({
      specPhaseCheckpoint: {
        featureName: 'web-clock',
        phase: 'design',
        artifactPath: '/w/.kiro/specs/web-clock/design.md',
        comments: [],
        review: null,
      },
    });
    await opening;

    // requirements.md must not be shown under the design checkpoint.
    expect(store.getState().specPhaseCheckpoint?.phase).toBe('design');
    expect(review(store)).toBeNull();
  });
});
