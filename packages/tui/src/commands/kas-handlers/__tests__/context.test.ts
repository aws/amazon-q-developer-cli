import { describe, it, expect, mock } from 'bun:test';
import { handleContext } from '../context';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { KasCommand } from '../../../kas-commands';
import { KasCommandName } from '../../../kas-commands';

const CONTEXT_CMD: KasCommand = {
  name: KasCommandName.Context,
  description: 'Show or manage context files',
  meta: {
    inputType: 'panel',
    subcommands: ['show', 'add', 'remove', 'clear'],
  },
};

/**
 * Convenience: build a CommandContext with stubs for every typed
 * /context method on `kiro`. Defaults model the happy path (show
 * returns no entries; mutations report agent-level success).
 *
 * Pass `kiroOverrides` to replace any of the typed stubs for a given
 * test — they'll be merged on top of the defaults.
 */
function ctxWith(
  kiroOverrides: Record<string, unknown> = {}
): ReturnType<typeof createMockCommandContext> {
  const defaults = {
    contextShow: mock(() => Promise.resolve({ entries: [] })),
    contextAdd: mock(() => Promise.resolve({ success: true, message: '' })),
    contextRemove: mock(() => Promise.resolve({ success: true, message: '' })),
    contextClear: mock(() => Promise.resolve({ success: true, message: '' })),
    getCachedContextBreakdown: mock(() => null),
  };
  return createMockCommandContext({
    kasCommands: [CONTEXT_CMD],
    kiro: { ...defaults, ...kiroOverrides } as any,
  });
}

describe('handleContext (KAS-mode dispatch)', () => {
  // ── show: bare and explicit ─────────────────────────────────────────

  describe('show flow', () => {
    it('bare /context with cached breakdown opens the panel collapsed', async () => {
      const ctx = ctxWith({
        getCachedContextBreakdown: mock(() => ({
          contextFiles: { tokens: 100, percent: 5 },
        })),
      });
      await handleContext(CONTEXT_CMD, '', ctx);

      const setBreakdown = ctx._spies.setShowContextBreakdown as any;
      expect(setBreakdown).toHaveBeenCalled();
      const [shown, breakdown] = setBreakdown.mock.calls[0];
      expect(shown).toBe(true);
      expect(breakdown.initialExpanded).toBe(false);
      // Always round-trips now (the show response carries a fresh breakdown);
      // the cache is only a fallback when the agent omits one.
      expect((ctx.kiro.contextShow as any).mock.calls.length).toBe(1);
    });

    it('prefers the show-response breakdown over the cached one', async () => {
      // The cache is stale (e.g. right after an /agent switch); the fresh
      // show-response breakdown must win.
      const ctx = ctxWith({
        getCachedContextBreakdown: mock(() => ({
          contextFiles: { tokens: 1, percent: 1, items: [{ name: 'stale' }] },
        })),
        contextShow: mock(() =>
          Promise.resolve({
            entries: [],
            breakdown: {
              contextFiles: {
                tokens: 999,
                percent: 9,
                items: [{ name: 'fresh' }],
              },
            },
          })
        ),
      });
      await handleContext(CONTEXT_CMD, 'show', ctx);

      const setBreakdown = ctx._spies.setShowContextBreakdown as any;
      expect(setBreakdown).toHaveBeenCalled();
      const breakdown = setBreakdown.mock.calls[0][1];
      expect(breakdown.contextFiles.items[0].name).toBe('fresh');
      expect(breakdown.initialExpanded).toBe(true);
    });

    it('/context show with cached breakdown opens the panel expanded', async () => {
      const ctx = ctxWith({
        getCachedContextBreakdown: mock(() => ({
          contextFiles: { tokens: 100, percent: 5 },
        })),
      });
      await handleContext(CONTEXT_CMD, 'show', ctx);

      const setBreakdown = ctx._spies.setShowContextBreakdown as any;
      const breakdown = setBreakdown.mock.calls[0][1];
      expect(breakdown.initialExpanded).toBe(true);
    });

    it('falls through to contextShow() when no breakdown is cached', async () => {
      const ctx = ctxWith({
        getCachedContextBreakdown: mock(() => null),
        contextShow: mock(() =>
          Promise.resolve({
            entries: [
              { path: 'foo.ts', matched: true },
              { path: 'bar.md', matched: true },
            ],
          })
        ),
      });
      await handleContext(CONTEXT_CMD, 'show', ctx);

      expect((ctx.kiro.contextShow as any).mock.calls.length).toBe(1);
      expect(ctx._spies.setShowContextBreakdown).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls[0][0])).toContain('foo.ts');
      expect(String(showAlert.mock.calls[0][0])).toContain('bar.md');
      expect(showAlert.mock.calls[0][1]).toBe('success');
    });

    it('warns on unmatched entries', async () => {
      const ctx = ctxWith({
        contextShow: mock(() =>
          Promise.resolve({
            entries: [
              { path: 'foo.ts', matched: true },
              { path: 'ghost.ts', matched: false },
            ],
          })
        ),
      });
      await handleContext(CONTEXT_CMD, 'show', ctx);

      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls[0][0])).toContain('⚠ ghost.ts');
      expect(showAlert.mock.calls[0][1]).toBe('warning');
    });

    it('shows soft warning when there are no entries and no cache', async () => {
      const ctx = ctxWith({
        contextShow: mock(() =>
          Promise.resolve({
            entries: [],
            message: 'No context files attached',
          })
        ),
      });
      await handleContext(CONTEXT_CMD, 'show', ctx);

      expect(ctx._spies.setShowContextBreakdown).not.toHaveBeenCalled();
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        'No context files attached',
        'warning',
        3000
      );
    });

    it('surfaces RPC failures from contextShow as an error alert', async () => {
      const ctx = ctxWith({
        contextShow: mock(() => Promise.reject(new Error('agent down'))),
      });
      await handleContext(CONTEXT_CMD, 'show', ctx);

      expect(ctx._spies.setShowContextBreakdown).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls[0][1]).toBe('error');
      expect(String(showAlert.mock.calls[0][0])).toContain('agent down');
    });
  });

  // ── add ─────────────────────────────────────────────────────────────

  describe('add', () => {
    it('routes /context add <path> to contextAdd with no force flag', async () => {
      const ctx = ctxWith({
        contextAdd: mock(() =>
          Promise.resolve({
            success: true,
            message: "Added 'foo.ts' to context",
          })
        ),
      });
      await handleContext(CONTEXT_CMD, 'add foo.ts', ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls[0]).toEqual([
        'foo.ts',
        { force: false },
      ]);
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        "Added 'foo.ts' to context",
        'success',
        3000
      );
      expect(ctx._spies.setShowContextBreakdown).not.toHaveBeenCalled();
    });

    it('forwards --force as { force: true }', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add --force tool-output.json', ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls[0]).toEqual([
        'tool-output.json',
        { force: true },
      ]);
    });

    it('also accepts the short -f flag for force', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add -f tool-output.json', ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls[0]).toEqual([
        'tool-output.json',
        { force: true },
      ]);
    });

    it('strips surrounding double quotes around paths with spaces', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add "my notes.md"', ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls[0][0]).toBe('my notes.md');
    });

    it('strips surrounding single quotes too', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, "add 'my notes.md'", ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls[0][0]).toBe('my notes.md');
    });

    it('shows a Usage error when no path is given (no agent call)', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add', ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls[0][0])).toContain('Usage');
      expect(showAlert.mock.calls[0][1]).toBe('error');
    });

    it('surfaces agent-level success=false as an error alert', async () => {
      const ctx = ctxWith({
        contextAdd: mock(() =>
          Promise.resolve({
            success: false,
            message: 'Path not found: ghost.ts',
          })
        ),
      });
      await handleContext(CONTEXT_CMD, 'add ghost.ts', ctx);

      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls[0][1]).toBe('error');
      expect(String(showAlert.mock.calls[0][0])).toContain('Path not found');
    });

    it('falls back to "Done" when agent omits a message', async () => {
      const ctx = ctxWith({
        contextAdd: mock(() => Promise.resolve({ success: true, message: '' })),
      });
      await handleContext(CONTEXT_CMD, 'add foo.ts', ctx);

      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls[0][0]).toBe('Done');
    });

    it('surfaces RPC errors from contextAdd as an error alert', async () => {
      const ctx = ctxWith({
        contextAdd: mock(() => Promise.reject(new Error('agent down'))),
      });
      await handleContext(CONTEXT_CMD, 'add foo.ts', ctx);

      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls[0][1]).toBe('error');
    });
  });

  // ── remove (incl. rm alias) ─────────────────────────────────────────

  describe('remove', () => {
    it('routes /context remove <path> to contextRemove', async () => {
      const ctx = ctxWith({
        contextRemove: mock(() =>
          Promise.resolve({
            success: true,
            message: "Removed 'foo.ts' from context",
          })
        ),
      });
      await handleContext(CONTEXT_CMD, 'remove foo.ts', ctx);

      expect((ctx.kiro.contextRemove as any).mock.calls[0]).toEqual(['foo.ts']);
    });

    it('aliases `rm` to remove', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'rm foo.ts', ctx);

      // Resolved client-side now (the handler owns the alias) so
      // contextRemove is invoked directly.
      expect((ctx.kiro.contextRemove as any).mock.calls[0]).toEqual(['foo.ts']);
      expect((ctx.kiro.contextAdd as any).mock.calls.length).toBe(0);
    });

    it('drops --force on remove (the flag is consumed but ignored)', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'remove --force foo.ts', ctx);

      // The force token is filtered from the positional list before
      // we build the path, so the path is still resolved correctly
      // and the typed signature stays clean.
      expect((ctx.kiro.contextRemove as any).mock.calls[0]).toEqual(['foo.ts']);
    });

    it('shows a Usage error when no path is given', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'remove', ctx);

      expect((ctx.kiro.contextRemove as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls[0][0])).toContain('Usage');
    });
  });

  // ── clear ───────────────────────────────────────────────────────────

  describe('clear', () => {
    it('routes /context clear to contextClear', async () => {
      const ctx = ctxWith({
        contextClear: mock(() =>
          Promise.resolve({
            success: true,
            message: 'Cleared 3 context entries',
          })
        ),
      });
      await handleContext(CONTEXT_CMD, 'clear', ctx);

      expect((ctx.kiro.contextClear as any).mock.calls.length).toBe(1);
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        'Cleared 3 context entries',
        'success',
        3000
      );
    });
  });

  // ── unknown subcommand ──────────────────────────────────────────────

  describe('unknown subcommand', () => {
    it('errors without invoking any typed method', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'wat', ctx);

      expect((ctx.kiro.contextShow as any).mock.calls.length).toBe(0);
      expect((ctx.kiro.contextAdd as any).mock.calls.length).toBe(0);
      expect((ctx.kiro.contextRemove as any).mock.calls.length).toBe(0);
      expect((ctx.kiro.contextClear as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls[0][1]).toBe('error');
      expect(String(showAlert.mock.calls[0][0])).toContain('Unknown');
    });
  });
});
