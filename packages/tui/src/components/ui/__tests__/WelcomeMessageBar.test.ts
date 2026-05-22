import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from '../../../stores/app-store';
import { Kiro } from '../../../kiro';

mock.module('../../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

describe('WelcomeMessageBar store integration', () => {
  it('renders nothing when announcement is null', () => {
    const store = createAppStore({ kiro: new Kiro() });
    expect(store.getState().announcement).toBeNull();
  });

  it('setAnnouncement stores id and maxLines', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({
      id: 'test',
      maxLines: 1,
    });

    const { announcement } = store.getState();
    expect(announcement).toEqual({ id: 'test', maxLines: 1 });
  });

  it('announcementExpanded defaults to false', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({
      id: 'test',
      maxLines: 1,
    });

    expect(store.getState().announcementExpanded).toBe(false);
  });

  it('toggleAnnouncementExpanded flips the state', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({
      id: 'test',
      maxLines: 1,
    });
    store.getState().toggleAnnouncementExpanded();

    expect(store.getState().announcementExpanded).toBe(true);

    store.getState().toggleAnnouncementExpanded();
    expect(store.getState().announcementExpanded).toBe(false);
  });

  it('setAnnouncement(null) clears announcement', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({ id: 'test', maxLines: 3 });
    store.getState().setAnnouncement(null);

    expect(store.getState().announcement).toBeNull();
  });
});
