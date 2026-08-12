import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import { createAppStore } from '../app-store';
import { detailBodyAt } from '../../utils/spec-artifact-parser/index.js';
import { commentsForDocument } from '../app-store';

/**
 * Tests for the artifactView slice navigation state machine.
 *
 * We exercise the public actions on a real store instance against a
 * temp-dir workspace so the loader has actual files to read.
 */

function makeFakeKiro(): any {
  return {
    initialize: () => Promise.resolve(),
    close: () => {},
    sessionId: 'test',
    settings: {},
    onCommandsUpdate: () => {},
    onKasCommandsDiscovered: () => {},
    onPromptsUpdate: () => {},
    onModelUpdate: () => {},
    onAgentUpdate: () => {},
    onCompactionStatus: () => {},
    onTurnSummary: () => {},
    onInitNotification: () => {},
    onHistoryEvent: () => {},
    onArtifactWrite: () => {},
    onSubagentListUpdate: () => {},
    onSessionEvent: () => {},
    onMultiSessionUpdate: () => {},
  };
}

let workspace: string;
let originalCwd: string;

beforeEach(() => {
  workspace = mkdtempSync(join(tmpdir(), 'artifact-view-test-'));
  originalCwd = process.cwd();
  process.chdir(workspace);
});

afterEach(() => {
  process.chdir(originalCwd);
  rmSync(workspace, { recursive: true, force: true });
});

function makeSpec(featureName: string, files: Record<string, string>): void {
  const dir = join(workspace, '.kiro', 'specs', featureName);
  mkdirSync(dir, { recursive: true });
  for (const [name, content] of Object.entries(files)) {
    writeFileSync(join(dir, name), content);
  }
}

describe('artifactView slice — openArtifactView', () => {
  it('opens a panel with the loaded summary on success', async () => {
    makeSpec('login', {
      'requirements.md': [
        '# Requirements',
        '### Requirement 1: First',
        '**User Story:** us-1',
        '### Requirement 2: Second',
        '**User Story:** us-2',
      ].join('\n'),
    });

    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('login', 'requirements');

    const view = store.getState().artifactViewOpen;
    expect(view).not.toBeNull();
    expect(view!.featureName).toBe('login');
    expect(view!.artifact).toBe('requirements');
    expect(view!.cursor).toBe(0);
    expect(view!.error).toBeNull();
    if (view!.summary.kind !== 'requirements') throw new Error('kind mismatch');
    expect(view!.summary.items).toHaveLength(2);
  });

  it('opens in error mode when feature is missing', async () => {
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('nonexistent', 'requirements');

    const view = store.getState().artifactViewOpen;
    expect(view).not.toBeNull();
    expect(view!.error).not.toBeNull();
    expect(view!.error!.message).toMatch(/no spec found/i);
  });

  it('opens in error mode when artifact file is missing', async () => {
    makeSpec('partial', { 'requirements.md': '' }); // tasks.md missing
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('partial', 'tasks');

    const view = store.getState().artifactViewOpen;
    expect(view).not.toBeNull();
    expect(view!.error).not.toBeNull();
  });
});

describe('artifactView slice — cursor navigation', () => {
  it('wraps backward from first to last', async () => {
    makeSpec('a', {
      'requirements.md': [
        '### Requirement 1: A',
        '### Requirement 2: B',
        '### Requirement 3: C',
      ].join('\n'),
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('a', 'requirements');

    expect(store.getState().artifactViewOpen!.cursor).toBe(0);
    store.getState().moveArtifactCursor('prev');
    expect(store.getState().artifactViewOpen!.cursor).toBe(2);
  });

  it('wraps forward from last to first', async () => {
    makeSpec('a', {
      'requirements.md': [
        '### Requirement 1: A',
        '### Requirement 2: B',
        '### Requirement 3: C',
      ].join('\n'),
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('a', 'requirements');
    // Move forward until we wrap.
    store.getState().moveArtifactCursor('next');
    store.getState().moveArtifactCursor('next');
    expect(store.getState().artifactViewOpen!.cursor).toBe(2);
    store.getState().moveArtifactCursor('next');
    expect(store.getState().artifactViewOpen!.cursor).toBe(0);
  });

  it('is a no-op for empty list', async () => {
    makeSpec('empty', { 'requirements.md': '# Requirements\n' });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('empty', 'requirements');

    expect(store.getState().artifactViewOpen!.cursor).toBe(0);
    store.getState().moveArtifactCursor('next');
    expect(store.getState().artifactViewOpen!.cursor).toBe(0);
    store.getState().moveArtifactCursor('prev');
    expect(store.getState().artifactViewOpen!.cursor).toBe(0);
  });

  it('cursor is preserved when toggling expand', async () => {
    makeSpec('t', {
      'tasks.md': ['- [ ] 1. A', '  - [ ] 1.1. A1', '- [ ] 2. B'].join('\n'),
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('t', 'tasks');

    store.getState().moveArtifactCursor('next');
    expect(store.getState().artifactViewOpen!.cursor).toBe(1);
    store.getState().toggleArtifactExpand(0);
    // Cursor unchanged; expand state for index 0 toggled.
    expect(store.getState().artifactViewOpen!.cursor).toBe(1);
    expect(store.getState().artifactViewOpen!.expanded[0]).toBe(true);
  });
});

describe('artifactView slice — expand/collapse', () => {
  it('toggle round-trips back to collapsed', async () => {
    makeSpec('t', {
      'tasks.md': '- [ ] 1. A\n  - [ ] 1.1. A1',
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('t', 'tasks');

    expect(!!store.getState().artifactViewOpen!.expanded[0]).toBe(false);
    store.getState().toggleArtifactExpand(0);
    expect(store.getState().artifactViewOpen!.expanded[0]).toBe(true);
    store.getState().toggleArtifactExpand(0);
    expect(store.getState().artifactViewOpen!.expanded[0]).toBe(false);
  });

  it('expansion state resets when panel is closed and reopened', async () => {
    makeSpec('t', {
      'tasks.md': '- [ ] 1. A\n  - [ ] 1.1. A1',
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('t', 'tasks');
    store.getState().toggleArtifactExpand(0);
    expect(store.getState().artifactViewOpen!.expanded[0]).toBe(true);

    store.getState().closeArtifactView();
    expect(store.getState().artifactViewOpen).toBeNull();

    await store.getState().openArtifactView('t', 'tasks');
    expect(store.getState().artifactViewOpen!.expanded[0]).toBeUndefined();
  });
});

describe('artifactView slice — opening the document at an item', () => {
  it('lands the review cursor on the item the summary cursor was on', async () => {
    makeSpec('a', {
      'requirements.md': ['### Requirement 1: A', '### Requirement 2: B'].join(
        '\n'
      ),
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('a', 'requirements');
    store.getState().moveArtifactCursor('next');

    const view = store.getState().artifactViewOpen!;
    await store
      .getState()
      .openSpecReview(
        'a',
        'requirements',
        detailBodyAt(view.summary, view.cursor)
      );

    const review = store.getState().specReviewView!;
    expect(review.document).toBe('requirements');
    expect(review.cursor.lineIndex).toBe(1);
    expect(review.lines[1]).toBe('### Requirement 2: B');
  });

  it('locates the item in the text it just read, not in a stale snapshot', async () => {
    makeSpec('a', {
      'requirements.md': ['### Requirement 1: A', '### Requirement 2: B'].join(
        '\n'
      ),
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('a', 'requirements');
    const view = store.getState().artifactViewOpen!;
    const slice = detailBodyAt(view.summary, 1);

    // The document grows a preamble while the panel sits open, so the item's
    // line in the panel's snapshot is no longer its line in the file.
    makeSpec('a', {
      'requirements.md': [
        '# Requirements',
        '',
        '### Requirement 1: A',
        '### Requirement 2: B',
      ].join('\n'),
    });
    await store.getState().openSpecReview('a', 'requirements', slice);

    const review = store.getState().specReviewView!;
    expect(review.cursor.lineIndex).toBe(3);
    expect(review.lines[review.cursor.lineIndex]).toBe('### Requirement 2: B');
  });

  it('parks another document’s comments instead of discarding them', async () => {
    makeSpec('a', {
      'requirements.md': '### Requirement 1: A',
      'design.md': '## Overview',
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openSpecReview('a', 'requirements', null);
    store.getState().startSpecReviewComment();
    store.getState().commitSpecReviewComment('tighten this');

    // Opening another document for review must not throw away what was typed
    // against the first: comments are hand-made, and only sending or deleting
    // them should remove them.
    await store.getState().openSpecReview('a', 'design', null);

    expect(
      commentsForDocument(store.getState(), 'a', 'requirements')
    ).toHaveLength(1);
    expect(commentsForDocument(store.getState(), 'a', 'design')).toHaveLength(
      0
    );

    // And they are still there to send when the user comes back to it.
    await store.getState().openSpecReview('a', 'requirements', null);
    expect(
      commentsForDocument(store.getState(), 'a', 'requirements')
    ).toHaveLength(1);
  });

  it('offers a document’s comments only to that document', async () => {
    makeSpec('a', {
      'requirements.md': '### Requirement 1: A',
      'design.md': '## Overview',
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openSpecReview('a', 'requirements', null);
    store.getState().startSpecReviewComment();
    store.getState().commitSpecReviewComment('tighten this');

    // They quote lines requirements.md has and design.md does not.
    expect(commentsForDocument(store.getState(), 'a', 'design')).toHaveLength(
      0
    );
    expect(
      commentsForDocument(store.getState(), 'b', 'requirements')
    ).toHaveLength(0);
  });

  it('drops only the sent document’s comments', async () => {
    makeSpec('a', {
      'requirements.md': '### Requirement 1: A',
      'design.md': '## Overview',
    });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openSpecReview('a', 'requirements', null);
    store.getState().startSpecReviewComment();
    store.getState().commitSpecReviewComment('tighten this');
    await store.getState().openSpecReview('a', 'design', null);
    store.getState().startSpecReviewComment();
    store.getState().commitSpecReviewComment('say why');

    store.getState().clearSpecReview('a', 'requirements');

    expect(
      commentsForDocument(store.getState(), 'a', 'requirements')
    ).toHaveLength(0);
    expect(commentsForDocument(store.getState(), 'a', 'design')).toHaveLength(
      1
    );
  });

  it('leaves the panel open underneath, so closing the review returns to it', async () => {
    makeSpec('a', { 'requirements.md': '### Requirement 1: A' });
    const store = createAppStore({ kiro: makeFakeKiro() });
    await store.getState().openArtifactView('a', 'requirements');
    await store.getState().openSpecReview('a', 'requirements', null);

    store.getState().closeSpecReview();

    expect(store.getState().specReviewView).toBeNull();
    expect(store.getState().artifactViewOpen).not.toBeNull();
  });
});

describe('artifactView slice — generation tracker', () => {
  it('creates an entry on first write', () => {
    const store = createAppStore({ kiro: makeFakeKiro() });
    const path = '/fake/.kiro/specs/x/requirements.md';
    store.getState().notifyArtifactGenerationWrite({
      path,
      featureName: 'x',
      artifact: 'requirements',
    });
    const entry = store.getState().artifactGenerating;
    expect(entry).not.toBeNull();
    expect(entry!.absolutePath).toBe(path);
    expect(entry!.featureName).toBe('x');
    expect(entry!.artifact).toBe('requirements');
    expect(entry!.complete).toBe(false);
  });

  it('markArtifactGenerationComplete flips complete to true', () => {
    const store = createAppStore({ kiro: makeFakeKiro() });
    const path = '/fake/.kiro/specs/x/tasks.md';
    store.getState().notifyArtifactGenerationWrite({
      path,
      featureName: 'x',
      artifact: 'tasks',
    });
    store.getState().markArtifactGenerationComplete(path);
    expect(store.getState().artifactGenerating!.complete).toBe(true);
  });

  it('a write to a different path replaces the active entry', () => {
    const store = createAppStore({ kiro: makeFakeKiro() });
    store.getState().notifyArtifactGenerationWrite({
      path: '/fake/.kiro/specs/a/requirements.md',
      featureName: 'a',
      artifact: 'requirements',
    });
    store.getState().notifyArtifactGenerationWrite({
      path: '/fake/.kiro/specs/b/design.md',
      featureName: 'b',
      artifact: 'design',
    });
    const entry = store.getState().artifactGenerating;
    expect(entry).not.toBeNull();
    expect(entry!.featureName).toBe('b');
    expect(entry!.artifact).toBe('design');
  });

  it('markArtifactGenerationComplete is a no-op for a non-active path', () => {
    const store = createAppStore({ kiro: makeFakeKiro() });
    const path = '/fake/.kiro/specs/x/requirements.md';
    store.getState().notifyArtifactGenerationWrite({
      path,
      featureName: 'x',
      artifact: 'requirements',
    });
    // Mark a different path complete — should not affect the active entry.
    store
      .getState()
      .markArtifactGenerationComplete('/fake/.kiro/specs/y/design.md');
    expect(store.getState().artifactGenerating!.complete).toBe(false);
  });

  it('clearArtifactViewOnEngineSwitch wipes both fields', () => {
    const store = createAppStore({ kiro: makeFakeKiro() });
    store.getState().notifyArtifactGenerationWrite({
      path: '/fake/x/requirements.md',
      featureName: 'x',
      artifact: 'requirements',
    });
    store.getState().clearArtifactViewOnEngineSwitch();
    expect(store.getState().artifactGenerating).toBeNull();
    expect(store.getState().artifactViewOpen).toBeNull();
  });
});
