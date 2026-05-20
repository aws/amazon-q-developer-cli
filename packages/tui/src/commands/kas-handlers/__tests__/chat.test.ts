import { describe, it, expect, mock } from 'bun:test';
import { handleChat } from '../chat';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { KasCommand } from '../../../kas-commands';
import { KasCommandName } from '../../../kas-commands';

const CHAT_CMD: KasCommand = {
  name: KasCommandName.Chat,
  description: 'Load a previous session or start a new one',
  meta: { inputType: 'selection', local: true },
};

describe('handleChat (KAS-mode dispatch)', () => {
  describe('list (no args)', () => {
    it('opens picker with sessions excluding the current one', async () => {
      const sessions = [
        {
          sessionId: 'aaaa1111',
          cwd: '/x',
          title: 'Other',
          updatedAt: new Date().toISOString(),
        },
        {
          sessionId: 'bbbb2222',
          cwd: '/x',
          title: 'Current',
          updatedAt: new Date().toISOString(),
        },
      ];
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          sessionId: 'bbbb2222',
          listSessions: mock(() =>
            Promise.resolve({ sessions, nextCursor: undefined })
          ),
        } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const setActive = ctx._spies.setActiveCommand as any;
      expect(setActive).toHaveBeenCalled();
      const arg = setActive.mock.calls[0][0];
      expect(arg.options.map((o: any) => o.value)).toEqual(['aaaa1111']);
    });

    it('alerts when no other sessions exist', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          sessionId: 'cur',
          listSessions: mock(() =>
            Promise.resolve({ sessions: [], nextCursor: undefined })
          ),
        } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalledWith(
        'No previous sessions found',
        'error',
        3000
      );
    });

    it('alerts on listSessions failure', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: {
          sessionId: 'cur',
          listSessions: mock(() => Promise.reject(new Error('boom'))),
        } as any,
      });
      await handleChat(CHAT_CMD, '', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalled();
      expect(showAlert.mock.calls[0][1]).toBe('error');
    });
  });

  describe('save / load (deferred verbs)', () => {
    it("alerts 'not yet supported' on save", async () => {
      const ctx = createMockCommandContext({ kasCommands: [CHAT_CMD] });
      await handleChat(CHAT_CMD, 'save /tmp/x.json', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalled();
      expect(String(showAlert.mock.calls[0][0])).toContain('not yet supported');
    });

    it("alerts 'not yet supported' on load", async () => {
      const ctx = createMockCommandContext({ kasCommands: [CHAT_CMD] });
      await handleChat(CHAT_CMD, 'load /tmp/x.json', ctx);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalled();
      expect(String(showAlert.mock.calls[0][0])).toContain('not yet supported');
    });
  });

  describe('unknown subcommand (typed directly)', () => {
    it('alerts and does not call loadSession', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      // No `argIsSynthetic` flag - simulates the user typing `/chat foo`
      // directly rather than picking from the picker.
      await handleChat(CHAT_CMD, 'foo', ctx);
      expect((loadSession as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalled();
      expect(String(showAlert.mock.calls[0][0])).toContain(
        'Unknown /chat subcommand'
      );
    });
  });

  describe('new / switch (self-contained, calls kiro directly)', () => {
    it('new: calls kiro.newSession + clears UI + resets messages', async () => {
      const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect((newSession as any).mock.calls.length).toBe(1);
      expect(ctx._spies.clearUIState).toHaveBeenCalled();
      expect(ctx._spies.resetMessages).toHaveBeenCalled();
      expect(ctx._spies.setSessionId).toHaveBeenCalledWith('newSID');
    });

    it('new <prompt>: forwards the prompt via sendMessage', async () => {
      const newSession = mock(() => Promise.resolve({ sessionId: 'newSID' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new hello there', ctx);
      expect(ctx._spies.sendMessage).toHaveBeenCalledWith('hello there');
    });

    it('new: alerts on failure without leaving loading state', async () => {
      const newSession = mock(() => Promise.reject(new Error('boom')));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { newSession } as any,
      });
      await handleChat(CHAT_CMD, 'new', ctx);
      expect(ctx._spies.setLoadingMessage).toHaveBeenLastCalledWith(null);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls.at(-1)?.[1]).toBe('error');
    });

    it('bare sessionId: calls kiro.loadSession + clears UI', async () => {
      const loadSession = mock(() => Promise.resolve({ sessionId: 'sid' }));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect((loadSession as any).mock.calls.length).toBe(1);
      expect((loadSession as any).mock.calls[0][0]).toBe('sid');
      expect(ctx._spies.clearUIState).toHaveBeenCalled();
      expect(ctx._spies.setSessionId).toHaveBeenCalledWith('sid');
    });

    it('bare sessionId: alerts on loadSession failure', async () => {
      const loadSession = mock(() => Promise.reject(new Error('nope')));
      const ctx = createMockCommandContext({
        kasCommands: [CHAT_CMD],
        kiro: { loadSession } as any,
      });
      await handleChat(CHAT_CMD, 'sid', ctx, { argIsSynthetic: true });
      expect(ctx._spies.setLoadingMessage).toHaveBeenLastCalledWith(null);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls.at(-1)?.[1]).toBe('error');
    });
  });
});
