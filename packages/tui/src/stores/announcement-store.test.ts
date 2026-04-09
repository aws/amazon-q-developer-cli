import { describe, it, expect, mock } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

describe('Announcement store state', () => {
  it('initializes with announcement: null', () => {
    const store = createAppStore({ kiro: new Kiro() });
    expect(store.getState().announcement).toBeNull();
    expect(store.getState().announcementExpanded).toBe(false);
  });

  it('setAnnouncement sets the message', () => {
    const store = createAppStore({ kiro: new Kiro() });
    const msg = { id: 'test-1', content: 'Hello world', maxLines: 1 };

    store.getState().setAnnouncement(msg);

    expect(store.getState().announcement).toEqual(msg);
  });

  it('setAnnouncement(null) clears it', () => {
    const store = createAppStore({ kiro: new Kiro() });
    store.getState().setAnnouncement({ id: 'x', content: 'y', maxLines: 1 });
    store.getState().setAnnouncement(null);

    expect(store.getState().announcement).toBeNull();
  });

  it('toggleAnnouncementExpanded toggles the expanded state', () => {
    const store = createAppStore({ kiro: new Kiro() });

    expect(store.getState().announcementExpanded).toBe(false);
    store.getState().toggleAnnouncementExpanded();
    expect(store.getState().announcementExpanded).toBe(true);
    store.getState().toggleAnnouncementExpanded();
    expect(store.getState().announcementExpanded).toBe(false);
  });
});
