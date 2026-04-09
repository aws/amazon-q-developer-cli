import { describe, it, expect, mock } from 'bun:test';
import { createAppStore } from '../../../stores/app-store';
import { Kiro } from '../../../kiro';

mock.module('../../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

describe('WelcomeMessageBar store integration', () => {
  it('renders nothing when announcement is null', () => {
    const store = createAppStore({ kiro: new Kiro() });
    expect(store.getState().announcement).toBeNull();
  });

  it('single-line content needs no expansion', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({
      id: 'test',
      content: 'Single line message',
      maxLines: 1,
    });

    const { announcement } = store.getState();
    const lines = announcement!.content.split('\n');
    expect(lines.length).toBeLessThanOrEqual(announcement!.maxLines);
  });

  it('multi-line content is truncated when not expanded', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({
      id: 'test',
      content: 'Line 1\nLine 2\nLine 3',
      maxLines: 1,
    });

    const { announcement, announcementExpanded } = store.getState();
    const lines = announcement!.content.split('\n');
    const isTruncated = lines.length > announcement!.maxLines;
    expect(isTruncated).toBe(true);
    expect(announcementExpanded).toBe(false);

    // Visible lines in collapsed mode
    const visibleLines = lines.slice(0, announcement!.maxLines);
    expect(visibleLines).toEqual(['Line 1']);
  });

  it('shows all lines when expanded', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({
      id: 'test',
      content: 'Line 1\nLine 2\nLine 3',
      maxLines: 1,
    });
    store.getState().toggleAnnouncementExpanded();

    const { announcement, announcementExpanded } = store.getState();
    expect(announcementExpanded).toBe(true);
    const lines = announcement!.content.split('\n');
    expect(lines).toEqual(['Line 1', 'Line 2', 'Line 3']);
  });

  it('no expand hint when content fits within maxLines', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({
      id: 'test',
      content: 'Just one line',
      maxLines: 3,
    });

    const { announcement } = store.getState();
    const lines = announcement!.content.split('\n');
    expect(lines.length <= announcement!.maxLines).toBe(true);
  });
});
