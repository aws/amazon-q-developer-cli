import { describe, it, expect } from 'bun:test';
import { getPlaceholder } from '../getPlaceholder';
import { getActiveGlyphs } from '../../../hooks/useGlyphs';
import { InterruptMode } from '../../../constants/interrupt-mode';

const baseOpts = () => ({
  glyphs: getActiveGlyphs(),
  editingQueueIndex: null,
  pendingApproval: false,
  isShellEscape: false,
  isProcessing: false,
  isInitialized: true,
  pendingSteerContent: null,
  activeInterruptMode: InterruptMode.STEER,
  toggleHintLabel: 'Ctrl+S',
  agentName: undefined,
});

describe('getPlaceholder', () => {
  it('shows a name-specific hint while spec description collection is pending', () => {
    const result = getPlaceholder({
      ...baseOpts(),
      specDescriptionFeature: 'web clock',
    });
    expect(result).toContain('describe what "web clock" should do');
    expect(result).toContain('esc to cancel');
  });

  it('processing wins over the spec description hint (stale flag)', () => {
    const result = getPlaceholder({
      ...baseOpts(),
      specDescriptionFeature: 'web clock',
      isProcessing: true,
    });
    expect(result).toContain('Kiro is working');
  });

  it('default prompt without the pending feature', () => {
    const result = getPlaceholder(baseOpts());
    expect(result).toContain('ask a question or describe a task');
  });

  it('shows the tangent go-back hint when tangentName is set', () => {
    const out = getPlaceholder({
      ...baseOpts(),
      tangentName: 'experiment',
    } as any);

    expect(out).toContain('/tangent to go back');
    expect(out).toContain('/tangent ls to view');
  });

  it('shows the default hint (no tangent text) when tangentName is null', () => {
    const out = getPlaceholder({ ...baseOpts(), tangentName: null } as any);

    expect(out).not.toContain('/tangent to go back');
    expect(out).toContain('ask a question or describe a task');
  });

  const goalStatus = () => ({
    state: 'active',
    iteration: 1,
    maxIterations: 5,
    message: 'fix all the tests',
  });

  it('active goal while processing offers pause', () => {
    const out = getPlaceholder({
      ...baseOpts(),
      isProcessing: true,
      goalStatus: goalStatus(),
    });
    expect(out).toContain('Goal Active: fix all the tests');
    expect(out).toContain('Iteration 2/5');
    expect(out).toContain('Ctrl+C to pause');
  });

  it('goal set but idle (paused) offers resume and cancel', () => {
    const out = getPlaceholder({
      ...baseOpts(),
      goalStatus: goalStatus(),
    });
    expect(out).toContain('Goal Paused: fix all the tests');
    expect(out).toContain('type to resume');
    expect(out).toContain('Ctrl+C to cancel');
  });

  it('explicitly paused goal offers resume and cancel even mid-processing', () => {
    const out = getPlaceholder({
      ...baseOpts(),
      isProcessing: true,
      goalStatus: { ...goalStatus(), state: 'paused' },
    });
    expect(out).toContain('Goal Paused: fix all the tests');
    expect(out).toContain('Ctrl+C to cancel');
  });

  it('after a failed cancel the paused hint points at /goal clear, not the quit key', () => {
    const out = getPlaceholder({
      ...baseOpts(),
      goalStatus: { ...goalStatus(), state: 'paused' },
      goalCancelFailed: true,
    });
    expect(out).toContain('/goal clear to cancel');
    expect(out).not.toContain('Ctrl+C to cancel');
  });

  it('paused cancel hint uses the quit binding label', () => {
    const out = getPlaceholder({
      ...baseOpts(),
      goalStatus: { ...goalStatus(), state: 'paused' },
      quitLabel: 'ctrl+q',
    });
    expect(out).toContain('ctrl+q to cancel');
  });
});
