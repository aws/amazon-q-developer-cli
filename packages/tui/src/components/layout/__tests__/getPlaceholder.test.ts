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
});
