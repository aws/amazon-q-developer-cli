import { describe, it, expect, mock } from 'bun:test';
import { handleContext } from '../context';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { KasCommand } from '../../../kas-commands';
import { KasCommandName } from '../../../kas-commands';
import type { ContextBreakdownData } from '../../../types/context';

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
  kiroOverrides: Record<string, unknown> = {},
  getContextBreakdownCache: () => ContextBreakdownData | null = () => null
): ReturnType<typeof createMockCommandContext> {
  const defaults = {
    contextShow: mock(() => Promise.resolve({ entries: [] })),
    contextAdd: mock(() => Promise.resolve({ success: true, message: '' })),
    contextRemove: mock(() => Promise.resolve({ success: true, message: '' })),
    contextClear: mock(() => Promise.resolve({ success: true, message: '' })),
  };
  return createMockCommandContext({
    kasCommands: [CONTEXT_CMD],
    kiro: { ...defaults, ...kiroOverrides } as any,
    getContextBreakdownCache,
  });
}

function makeBreakdown(
  contextFiles: ContextBreakdownData['contextFiles']
): ContextBreakdownData {
  return {
    contextFiles,
    tools: { tokens: 20, percent: 1 },
    kiroResponses: { tokens: 30, percent: 2 },
    yourPrompts: { tokens: 40, percent: 2 },
  };
}

describe('handleContext (KAS-mode dispatch)', () => {
  // ── show: bare and explicit ─────────────────────────────────────────

  describe('show flow', () => {
    it('bare /context with cached breakdown opens the panel collapsed', async () => {
      const ctx = ctxWith({}, () => makeBreakdown({ tokens: 100, percent: 5 }));
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
      const ctx = ctxWith(
        {
          contextShow: mock(() =>
            Promise.resolve({
              entries: [],
              breakdown: makeBreakdown({
                tokens: 999,
                percent: 9,
                items: [
                  { name: 'fresh', tokens: 999, percent: 9, matched: true },
                ],
              }),
            })
          ),
        },
        () =>
          makeBreakdown({
            tokens: 1,
            percent: 1,
            items: [{ name: 'stale', tokens: 1, percent: 1, matched: true }],
          })
      );
      await handleContext(CONTEXT_CMD, 'show', ctx);

      const setBreakdown = ctx._spies.setShowContextBreakdown as any;
      expect(setBreakdown).toHaveBeenCalled();
      const breakdown = setBreakdown.mock.calls[0][1];
      expect(breakdown.contextFiles.items[0].name).toBe('fresh');
      expect(breakdown.initialExpanded).toBe(true);
    });

    it('/context show with cached breakdown opens the panel expanded', async () => {
      const ctx = ctxWith({}, () => makeBreakdown({ tokens: 100, percent: 5 }));
      await handleContext(CONTEXT_CMD, 'show', ctx);

      const setBreakdown = ctx._spies.setShowContextBreakdown as any;
      const breakdown = setBreakdown.mock.calls[0][1];
      expect(breakdown.initialExpanded).toBe(true);
    });

    it('uses a cache update that arrives while contextShow is pending', async () => {
      let resolveShow!: (response: { entries: [] }) => void;
      const showResponse = new Promise<{ entries: [] }>((resolve) => {
        resolveShow = resolve;
      });
      let cachedBreakdown: ContextBreakdownData | null = null;
      const ctx = ctxWith(
        {
          contextShow: mock(() => showResponse),
        },
        () => cachedBreakdown
      );

      const handling = handleContext(CONTEXT_CMD, 'show', ctx);
      cachedBreakdown = makeBreakdown({
        tokens: 777,
        percent: 7,
        items: [{ name: 'mid-flight', tokens: 777, percent: 7, matched: true }],
      });
      resolveShow({ entries: [] });
      await handling;

      const setBreakdown = ctx._spies.setShowContextBreakdown as any;
      expect(setBreakdown.mock.calls[0][1]).toMatchObject({
        contextFiles: {
          items: [{ name: 'mid-flight' }],
        },
        initialExpanded: true,
      });
    });

    it('falls through to contextShow() when no breakdown is cached', async () => {
      const ctx = ctxWith({
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

    // shellSplit already resolved quoting — the handler must NOT re-strip, or a
    // quote char that is genuinely part of the filename (via escape) gets eaten.
    it('keeps quote chars that are part of the literal filename (no double-strip)', async () => {
      const ctx = ctxWith();
      // \" \" escape a leading + trailing double-quote → filename is  "weird"
      await handleContext(CONTEXT_CMD, 'add \\"weird\\"', ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls[0][0]).toBe('"weird"');
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

    it('summarizes with a V2-style message even when agent omits one', async () => {
      const ctx = ctxWith({
        contextAdd: mock(() => Promise.resolve({ success: true, message: '' })),
      });
      await handleContext(CONTEXT_CMD, 'add foo.ts', ctx);

      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls[0][0]).toBe("Added 'foo.ts' to context");
      expect(showAlert.mock.calls[0][1]).toBe('success');
    });

    it('adds EACH path once for multi-path input (no collapse)', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add src/a.ts src/b.ts', ctx);

      const add = ctx.kiro.contextAdd as any;
      expect(add.mock.calls.length).toBe(2);
      expect(add.mock.calls[0]).toEqual(['src/a.ts', { force: false }]);
      expect(add.mock.calls[1]).toEqual(['src/b.ts', { force: false }]);
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        'Added 2 path(s) to context',
        'success',
        3000
      );
    });

    it('keeps quoted paths with spaces intact across multiple paths', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add "my a.md" "my b.md"', ctx);

      const add = ctx.kiro.contextAdd as any;
      expect(add.mock.calls.length).toBe(2);
      expect(add.mock.calls[0][0]).toBe('my a.md');
      expect(add.mock.calls[1][0]).toBe('my b.md');
    });

    it('applies --force to every path', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add --force a.ts b.ts', ctx);

      const add = ctx.kiro.contextAdd as any;
      expect(add.mock.calls[0]).toEqual(['a.ts', { force: true }]);
      expect(add.mock.calls[1]).toEqual(['b.ts', { force: true }]);
    });

    // V2 shell_split parity (crates/chat-cli-v2/.../commands/mod.rs): globs
    // stay literal for the agent to expand; backslash escapes a space.
    it('passes globs through literally as a single path', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add src/*.ts', ctx);

      const add = ctx.kiro.contextAdd as any;
      expect(add.mock.calls.length).toBe(1);
      expect(add.mock.calls[0][0]).toBe('src/*.ts');
    });

    it('honors a backslash-escaped space as one path', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add my\\ notes.md', ctx);

      const add = ctx.kiro.contextAdd as any;
      expect(add.mock.calls.length).toBe(1);
      expect(add.mock.calls[0][0]).toBe('my notes.md');
    });

    // V2 shell_split parity: an unclosed quote still yields the accumulated
    // token (quote char stripped), not a dropped/empty path.
    it('treats an unclosed quote as one path (quote stripped)', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add "unclosed path', ctx);

      const add = ctx.kiro.contextAdd as any;
      expect(add.mock.calls.length).toBe(1);
      expect(add.mock.calls[0][0]).toBe('unclosed path');
    });

    it('surfaces per-path failures AND reports the paths that did apply', async () => {
      const ctx = ctxWith({
        contextAdd: mock((p: string) =>
          Promise.resolve(
            p === 'ghost.ts'
              ? { success: false, message: 'Path not found: ghost.ts' }
              : { success: true, message: '' }
          )
        ),
      });
      await handleContext(CONTEXT_CMD, 'add ok.ts ghost.ts', ctx);

      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls[0][1]).toBe('error');
      const msg = String(showAlert.mock.calls[0][0]);
      // partial success must be visible: 1 path applied, ghost failed
      expect(msg).toContain('Added 1 path(s) to context');
      expect(msg).toContain('ghost.ts');
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

    it('removes EACH path once for multi-path input (no collapse)', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'remove a.ts b.ts', ctx);

      const rm = ctx.kiro.contextRemove as any;
      expect(rm.mock.calls.length).toBe(2);
      expect(rm.mock.calls[0]).toEqual(['a.ts']);
      expect(rm.mock.calls[1]).toEqual(['b.ts']);
      expect(ctx._spies.showAlert).toHaveBeenCalledWith(
        'Removed 2 path(s) from context',
        'success',
        3000
      );
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

  // ── cloud-session gate ──────────────────────────────────────────────

  describe('cloud-session gate (mutations refuse; show + local untouched)', () => {
    function cloudCtx(): ReturnType<typeof ctxWith> {
      const ctx = ctxWith();
      ctx.cloudSessionActive = true;
      return ctx;
    }

    it('cloud: add refuses with the exact message and never calls the RPC', async () => {
      const ctx = cloudCtx();
      await handleContext(CONTEXT_CMD, 'add /tmp/a.md', ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls).toEqual([
        [
          '/context add is not available for a cloud session yet.',
          'error',
          5000,
        ],
      ]);
    });

    it('cloud: remove refuses with the exact message and never calls the RPC', async () => {
      const ctx = cloudCtx();
      await handleContext(CONTEXT_CMD, 'remove /tmp/a.md', ctx);

      expect((ctx.kiro.contextRemove as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls).toEqual([
        [
          '/context remove is not available for a cloud session yet.',
          'error',
          5000,
        ],
      ]);
    });

    it('cloud: rm alias refuses with the remove wording', async () => {
      const ctx = cloudCtx();
      await handleContext(CONTEXT_CMD, 'rm /tmp/a.md', ctx);

      expect((ctx.kiro.contextRemove as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(String(showAlert.mock.calls[0][0])).toBe(
        '/context remove is not available for a cloud session yet.'
      );
    });

    it('cloud: clear refuses with the exact message and never calls the RPC', async () => {
      const ctx = cloudCtx();
      await handleContext(CONTEXT_CMD, 'clear', ctx);

      expect((ctx.kiro.contextClear as any).mock.calls.length).toBe(0);
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert.mock.calls).toEqual([
        [
          '/context clear is not available for a cloud session yet.',
          'error',
          5000,
        ],
      ]);
    });

    it('cloud: show stays available (read-only, no refusal)', async () => {
      const ctx = cloudCtx();
      await handleContext(CONTEXT_CMD, 'show', ctx);

      expect((ctx.kiro.contextShow as any).mock.calls.length).toBe(1);
      const showAlert = ctx._spies.showAlert as any;
      expect(
        showAlert.mock.calls.some((c: any[]) =>
          String(c[0]).includes('not available for a cloud session')
        )
      ).toBe(false);
    });

    it('local: add is untouched by the gate (RPC still runs, no refusal)', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'add /tmp/a.md', ctx);

      expect((ctx.kiro.contextAdd as any).mock.calls.length).toBe(1);
      const showAlert = ctx._spies.showAlert as any;
      expect(
        showAlert.mock.calls.some((c: any[]) =>
          String(c[0]).includes('not available for a cloud session')
        )
      ).toBe(false);
    });

    it('local: clear is untouched by the gate (RPC still runs, no refusal)', async () => {
      const ctx = ctxWith();
      await handleContext(CONTEXT_CMD, 'clear', ctx);

      expect((ctx.kiro.contextClear as any).mock.calls.length).toBe(1);
      const showAlert = ctx._spies.showAlert as any;
      expect(
        showAlert.mock.calls.some((c: any[]) =>
          String(c[0]).includes('not available for a cloud session')
        )
      ).toBe(false);
    });
  });
});
