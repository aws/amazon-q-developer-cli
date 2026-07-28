import { describe, it, expect, mock } from 'bun:test';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import { handleTangent } from '../tangent';
import type { KasCommand } from '../../../kas-commands';

const TAN_CMD: KasCommand = {
  name: '/tangent' as any,
  description: 'Tangent',
  meta: { inputType: 'panel' },
};

function mockKiro(overrides: Record<string, any> = {}) {
  return {
    sessionId: 'current-session',
    listSessions: mock(() =>
      Promise.resolve({
        sessions: [
          { sessionId: 'root', title: 'Main', parentSessionId: undefined },
          {
            sessionId: 'current-session',
            title: 'experiment',
            parentSessionId: 'root',
          },
          {
            sessionId: 'other',
            title: 'other-tangent',
            parentSessionId: 'root',
          },
        ],
      })
    ),
    loadSession: mock((_id: string) =>
      Promise.resolve({ currentModel: 'auto', currentAgent: null })
    ),
    executeCommand: mock(() =>
      Promise.resolve({
        success: true,
        message: '',
        data: { sessionId: 'new-fork-id' },
      })
    ),
    fork: mock(() =>
      Promise.resolve({
        success: true,
        message: '',
        data: { sessionId: 'new-fork-id' },
      })
    ),
    ...overrides,
  };
}

describe('handleTangent', () => {
  describe('/tangent (no args) — go back to parent', () => {
    it('switches to parent session when on a child', async () => {
      const kiro = mockKiro();
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, '', ctx as any);

      expect(kiro.loadSession).toHaveBeenCalledTimes(1);
      expect((kiro.loadSession as any).mock.calls[0][0]).toBe('root');
    });

    it('auto-creates tangent when on root (no parent)', async () => {
      const kiro = mockKiro({ sessionId: 'root' });
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, '', ctx as any);

      // Should fork (create new tangent) since we're on root
      expect(kiro.fork).toHaveBeenCalledTimes(1);
      const forkArg = (kiro.fork as any).mock.calls[0][0];
      expect(forkArg.createdReason).toBe('tangent');
      expect(forkArg.title).toBe('tangent-1');
      // Chip is set to the new tangent's name after auto-create
      const setTangentCalls = (ctx._spies.setTangentName as any).mock.calls;
      expect(setTangentCalls[setTangentCalls.length - 1][0]).toBe('tangent-1');
    });

    it('keeps the chip on the parent tangent when going back in a nested tree (root -> ddb -> ddb1)', async () => {
      // Regression: setTangentName must derive from the session list, not the
      // load response. loadSession here returns no parentSessionId (as older KAS
      // builds do), which previously cleared the chip and made ddb look like root.
      const kiro = mockKiro({
        sessionId: 'ddb1',
        listSessions: mock(() =>
          Promise.resolve({
            sessions: [
              { sessionId: 'root', title: 'Main', parentSessionId: undefined },
              { sessionId: 'ddb', title: 'ddb', parentSessionId: 'root' },
              { sessionId: 'ddb1', title: 'ddb1', parentSessionId: 'ddb' },
            ],
          })
        ),
      });
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, '', ctx as any);

      // Loaded the immediate parent (ddb), not root
      expect((kiro.loadSession as any).mock.calls[0][0]).toBe('ddb');
      // Chip shows 'ddb' (ddb is itself a tangent), NOT cleared
      const setTangentCalls = (ctx._spies.setTangentName as any).mock.calls;
      expect(setTangentCalls[setTangentCalls.length - 1][0]).toBe('ddb');
    });

    it('clears the chip when going back to root (root -> ec2, back to root)', async () => {
      const kiro = mockKiro({
        sessionId: 'ec2',
        listSessions: mock(() =>
          Promise.resolve({
            sessions: [
              { sessionId: 'root', title: 'Main', parentSessionId: undefined },
              { sessionId: 'ec2', title: 'ec2', parentSessionId: 'root' },
            ],
          })
        ),
      });
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, '', ctx as any);

      // Loaded root (ec2's parent); chip cleared to null since root is not a tangent
      expect((kiro.loadSession as any).mock.calls[0][0]).toBe('root');
      const setTangentCalls = (ctx._spies.setTangentName as any).mock.calls;
      expect(setTangentCalls[setTangentCalls.length - 1][0]).toBe(null);
    });
  });

  describe('/tangent ls — show tree picker', () => {
    it('opens tangent explorer with tree rows', async () => {
      const kiro = mockKiro();
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, 'ls', ctx as any);

      expect(ctx._spies.setShowTangentExplorer).toHaveBeenCalledTimes(1);
      const [show, rows] = (ctx._spies.setShowTangentExplorer as any).mock
        .calls[0];
      expect(show).toBe(true);
      expect(rows.length).toBeGreaterThan(1);
      expect(rows.some((r: any) => r.label.includes('experiment'))).toBe(true);
    });

    it('shows alert when no tangents exist', async () => {
      // The current session is always present in the list in reality; anchor at
      // root (whose tree has no tangent children). The old test relied on the
      // removed roots[0]-for-missing-anchor fallback.
      const kiro = mockKiro({ sessionId: 'root' });
      (kiro.listSessions as any).mockImplementation(() =>
        Promise.resolve({
          sessions: [{ sessionId: 'root', title: 'Main' }],
        })
      );
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, 'ls', ctx as any);

      expect(ctx._spies.showAlert).toHaveBeenCalledTimes(1);
      const alertCall = (ctx._spies.showAlert as any).mock.calls[0];
      expect(alertCall[0]).toContain('No tangents yet');
    });
  });

  describe('/tangent <name> — switch or create', () => {
    it('switches to existing tangent by name', async () => {
      const kiro = mockKiro();
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, 'other-tangent', ctx as any);

      expect(kiro.loadSession).toHaveBeenCalledTimes(1);
      expect((kiro.loadSession as any).mock.calls[0][0]).toBe('other');
    });

    it('creates new tangent when name does not exist', async () => {
      const kiro = mockKiro();
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, 'new-experiment', ctx as any);

      // Should fork first, then loadSession
      expect(kiro.fork).toHaveBeenCalledTimes(1);
      expect(kiro.loadSession).toHaveBeenCalledTimes(1);
      expect((kiro.loadSession as any).mock.calls[0][0]).toBe('new-fork-id');
    });
  });
});

describe('handleTangent — error recovery', () => {
  it('alerts and does not load/fork when listSessions fails (goBack path)', async () => {
    // KasAcpClient.listSessions never throws; it signals failure via
    // { sessions: [], failed: true }. A read failure must abort, not fall
    // through to an empty list (which would auto-create or fork a duplicate).
    const kiro = mockKiro({
      listSessions: mock(() => Promise.resolve({ sessions: [], failed: true })),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, '', ctx as any);

    expect(ctx._spies.showAlert).toHaveBeenCalledTimes(1);
    expect((ctx._spies.showAlert as any).mock.calls[0][1]).toBe('error');
    expect(kiro.loadSession).not.toHaveBeenCalled();
    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('aborts (no auto-create) when listSessions fails on root', async () => {
    // Without the failed-flag check, an empty list on root would auto-create
    // tangent-1 on a transient read failure; the fix must abort with an alert.
    const kiro = mockKiro({
      sessionId: 'root',
      listSessions: mock(() => Promise.resolve({ sessions: [], failed: true })),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, '', ctx as any);

    expect(kiro.fork).not.toHaveBeenCalled();
    expect((ctx._spies.showAlert as any).mock.calls[0][1]).toBe('error');
  });

  it('alerts and does not load when fork returns failure (create path)', async () => {
    const kiro = mockKiro({
      fork: mock(() =>
        Promise.resolve({
          success: false,
          message: 'quota exceeded',
          data: undefined,
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'brand-new', ctx as any);

    expect(kiro.fork).toHaveBeenCalledTimes(1);
    expect(kiro.loadSession).not.toHaveBeenCalled();
    expect((ctx._spies.showAlert as any).mock.calls[0][1]).toBe('error');
  });

  it('alerts (no crash) when fork throws an exception (create path)', async () => {
    const kiro = mockKiro({
      fork: mock(() => Promise.reject(new Error('network error'))),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'brand-new', ctx as any);

    expect(kiro.fork).toHaveBeenCalledTimes(1);
    expect(kiro.loadSession).not.toHaveBeenCalled();
    expect((ctx._spies.showAlert as any).mock.calls[0][1]).toBe('error');
  });

  it('alerts and does NOT reset/clear the view when loadSession throws (no blank screen)', async () => {
    const kiro = mockKiro({
      loadSession: mock(() => Promise.reject(new Error('load failed'))),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    // 'other-tangent' is an existing tangent -> switch path -> loadSession
    await handleTangent(TAN_CMD, 'other-tangent', ctx as any);

    expect(kiro.loadSession).toHaveBeenCalledTimes(1);
    expect((ctx._spies.showAlert as any).mock.calls[0][1]).toBe('error');
    // no-blank-screen guarantee: clear/reset must not run on a failed load
    expect(ctx._spies.resetMessages).not.toHaveBeenCalled();
    expect(ctx._spies.clearUIState).not.toHaveBeenCalled();
    expect(ctx._spies.setSessionId).not.toHaveBeenCalled();
  });
});

describe('handleTangent — ls subcommand is case-insensitive', () => {
  // 'root'/'Root'/'ROOT' are keywords that jump to the tree root; the 'ls'
  // picker subcommand is matched case-insensitively too, so uppercase variants
  // open the picker rather than erroring as a reserved name.
  for (const name of ['LS', 'Ls']) {
    it(`"${name}" opens the tangent picker (same as ls)`, async () => {
      const kiro = mockKiro();
      const ctx = createMockCommandContext({ kiro: kiro as any });

      await handleTangent(TAN_CMD, name, ctx as any);

      expect(ctx._spies.setShowTangentExplorer).toHaveBeenCalledTimes(1);
      expect((ctx._spies.setShowTangentExplorer as any).mock.calls[0][0]).toBe(
        true
      );
      expect(kiro.fork).not.toHaveBeenCalled();
      expect(kiro.loadSession).not.toHaveBeenCalled();
    });
  }
});

describe('handleTangent — auto-name counter', () => {
  it('auto-creates tangent-2 when tangent-1 already exists (on root)', async () => {
    const kiro = mockKiro({
      sessionId: 'root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            { sessionId: 'root', title: 'Main', parentSessionId: undefined },
            { sessionId: 't1', title: 'tangent-1', parentSessionId: 'root' },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, '', ctx as any);

    expect(kiro.fork).toHaveBeenCalledTimes(1);
    expect((kiro.fork as any).mock.calls[0][0].title).toBe('tangent-2');
  });

  it('numbers per root; ignores tangents under a different root', async () => {
    const kiro = mockKiro({
      sessionId: 'root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            { sessionId: 'root', title: 'Main', parentSessionId: undefined },
            {
              sessionId: 'otherRoot',
              title: 'Other',
              parentSessionId: undefined,
            },
            {
              sessionId: 'x1',
              title: 'tangent-1',
              parentSessionId: 'otherRoot',
            },
            {
              sessionId: 'x2',
              title: 'tangent-2',
              parentSessionId: 'otherRoot',
            },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, '', ctx as any);

    // 'root' has no tangent-* children, so numbering starts fresh at 1;
    // the tangent-1/2 under otherRoot must not count.
    expect(kiro.fork).toHaveBeenCalledTimes(1);
    expect((kiro.fork as any).mock.calls[0][0].title).toBe('tangent-1');
  });

  it('numbers against the whole tree, not just direct children (deep collision)', async () => {
    // root has direct children tangent-1 and tangent-2; a DEEPER descendant
    // (grandchild under t1) is titled tangent-3. Direct-children numbering
    // would pick tangent-3, and switchOrCreate's whole-tree lookup would then
    // find the deep one and silently SWITCH instead of creating. Whole-tree
    // numbering must skip tangent-3 and create a fresh, unique tangent-4.
    const kiro = mockKiro({
      sessionId: 'root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            { sessionId: 'root', title: 'Main', parentSessionId: undefined },
            { sessionId: 't1', title: 'tangent-1', parentSessionId: 'root' },
            { sessionId: 't2', title: 'tangent-2', parentSessionId: 'root' },
            // grandchild of root (child of t1) — NOT a direct child of root:
            { sessionId: 't3', title: 'tangent-3', parentSessionId: 't1' },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, '', ctx as any);

    // Must CREATE a new whole-tree-unique tangent (fork called), not switch
    // into the existing deep tangent-3.
    expect(kiro.fork).toHaveBeenCalledTimes(1);
    expect((kiro.fork as any).mock.calls[0][0].title).toBe('tangent-4');
  });

  it('creates tangent-100 when tangent-1..99 already exist', async () => {
    const children = Array.from({ length: 99 }, (_, i) => ({
      sessionId: `t${i + 1}`,
      title: `tangent-${i + 1}`,
      parentSessionId: 'root',
    }));
    const kiro = mockKiro({
      sessionId: 'root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            { sessionId: 'root', title: 'Main', parentSessionId: undefined },
            ...children,
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, '', ctx as any);

    expect(kiro.fork).toHaveBeenCalledTimes(1);
    expect((kiro.fork as any).mock.calls[0][0].title).toBe('tangent-100');
  });

  it('creates tangent-101 when tangent-1..100 all exist (unbounded, no cap)', async () => {
    // Numbering has no upper bound. The loop still terminates because the
    // collision set is finite: with tangent-1..100 taken, counter reaches the
    // first free number (101) and forks a new tangent rather than capping.
    const children = Array.from({ length: 100 }, (_, i) => ({
      sessionId: `t${i + 1}`,
      title: `tangent-${i + 1}`,
      parentSessionId: 'root',
    }));
    const kiro = mockKiro({
      sessionId: 'root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            { sessionId: 'root', title: 'Main', parentSessionId: undefined },
            ...children,
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, '', ctx as any);

    expect(kiro.fork).toHaveBeenCalledTimes(1);
    expect((kiro.fork as any).mock.calls[0][0].title).toBe('tangent-101');
  });
});

describe('handleTangent — case-insensitive switch by name', () => {
  it('switches to an existing tangent when the typed name differs in case', async () => {
    const kiro = mockKiro();
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'OTHER-TANGENT', ctx as any);

    expect(kiro.loadSession).toHaveBeenCalledTimes(1);
    expect((kiro.loadSession as any).mock.calls[0][0]).toBe('other');
    expect(kiro.fork).not.toHaveBeenCalled();
  });
});

describe('handleTangent — Explorer selection dispatches a session id', () => {
  const tree = () => ({
    sessions: [
      { sessionId: 'sess-root', title: 'Main', parentSessionId: undefined },
      { sessionId: 'sess-ec2', title: 'ec2', parentSessionId: 'sess-root' },
      { sessionId: 'sess-t3', title: 't3', parentSessionId: 'sess-ec2' },
    ],
  });

  it('switches to the exact selected session id (root) instead of going back one level', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-t3', // on a deep descendant
      listSessions: mock(() => Promise.resolve(tree())),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    // Explorer dispatches the root row's sessionId (not a title / bare command)
    await handleTangent(TAN_CMD, 'sess-root', ctx as any);

    expect(kiro.loadSession).toHaveBeenCalledTimes(1);
    expect((kiro.loadSession as any).mock.calls[0][0]).toBe('sess-root');
    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('no-ops when the selected session id is the current session', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-ec2',
      listSessions: mock(() => Promise.resolve(tree())),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'sess-ec2', ctx as any); // select the current row

    expect(kiro.loadSession).not.toHaveBeenCalled();
    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('still creates a new tangent for a typed name that is not a session id', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-ec2',
      listSessions: mock(() => Promise.resolve(tree())),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'brand-new', ctx as any);

    expect(kiro.fork).toHaveBeenCalledTimes(1);
    expect((kiro.fork as any).mock.calls[0][0].title).toBe('brand-new');
  });
});

describe('handleTangent — Explorer selection edge cases (switch by id)', () => {
  it('switches to a sibling tangent by id', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-ec2',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            {
              sessionId: 'sess-root',
              title: 'Main',
              parentSessionId: undefined,
            },
            {
              sessionId: 'sess-ec2',
              title: 'ec2',
              parentSessionId: 'sess-root',
            },
            {
              sessionId: 'sess-t2',
              title: 'tangent-2',
              parentSessionId: 'sess-root',
            },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'sess-t2', ctx as any);

    expect((kiro.loadSession as any).mock.calls[0][0]).toBe('sess-t2');
    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('switches to a deep descendant by id', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            {
              sessionId: 'sess-root',
              title: 'Main',
              parentSessionId: undefined,
            },
            {
              sessionId: 'sess-ec2',
              title: 'ec2',
              parentSessionId: 'sess-root',
            },
            { sessionId: 'sess-t3', title: 't3', parentSessionId: 'sess-ec2' },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'sess-t3', ctx as any);

    expect((kiro.loadSession as any).mock.calls[0][0]).toBe('sess-t3');
    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('switches to the exact selected id even when two tangents share a title', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            {
              sessionId: 'sess-root',
              title: 'Main',
              parentSessionId: undefined,
            },
            {
              sessionId: 'sess-d1',
              title: 'dup',
              parentSessionId: 'sess-root',
            },
            {
              sessionId: 'sess-d2',
              title: 'dup',
              parentSessionId: 'sess-root',
            },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'sess-d2', ctx as any);

    // Must load the id we selected, not the first title match.
    expect((kiro.loadSession as any).mock.calls[0][0]).toBe('sess-d2');
    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('switches by id even when the target has no parentSessionId (does not create)', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            {
              sessionId: 'sess-root',
              title: 'Main',
              parentSessionId: undefined,
            },
            // A tangent whose parent link is momentarily absent in the list.
            {
              sessionId: 'sess-orphan',
              title: 'orphan',
              parentSessionId: undefined,
            },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'sess-orphan', ctx as any);

    expect((kiro.loadSession as any).mock.calls[0][0]).toBe('sess-orphan');
    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('sets the chip to the tangent title when switching to a tangent by id', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            {
              sessionId: 'sess-root',
              title: 'Main',
              parentSessionId: undefined,
            },
            {
              sessionId: 'sess-ec2',
              title: 'ec2',
              parentSessionId: 'sess-root',
            },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'sess-ec2', ctx as any);

    const calls = (ctx._spies.setTangentName as any).mock.calls;
    expect(calls[calls.length - 1][0]).toBe('ec2');
  });

  it('clears the chip (null) when switching to root by id', async () => {
    const kiro = mockKiro({
      sessionId: 'sess-ec2',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            {
              sessionId: 'sess-root',
              title: 'Main',
              parentSessionId: undefined,
            },
            {
              sessionId: 'sess-ec2',
              title: 'ec2',
              parentSessionId: 'sess-root',
            },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'sess-root', ctx as any);

    const calls = (ctx._spies.setTangentName as any).mock.calls;
    expect(calls[calls.length - 1][0]).toBeNull();
  });
});

describe('handleTangent — /tangent root keyword', () => {
  it('jumps to the tree root from a deep nested tangent', async () => {
    const kiro = mockKiro({
      sessionId: 'deep-child',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            { sessionId: 'root', title: 'Main', parentSessionId: undefined },
            { sessionId: 'mid', title: 'mid-tangent', parentSessionId: 'root' },
            { sessionId: 'deep-child', title: 'deep', parentSessionId: 'mid' },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'root', ctx as any);

    // Loaded the tree root, NOT created a tangent named "root"
    expect(kiro.loadSession).toHaveBeenCalledTimes(1);
    expect((kiro.loadSession as any).mock.calls[0][0]).toBe('root');
    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('is case-insensitive (ROOT, Root)', async () => {
    const kiro = mockKiro({
      sessionId: 'current-session',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            { sessionId: 'root', title: 'Main', parentSessionId: undefined },
            {
              sessionId: 'current-session',
              title: 'exp',
              parentSessionId: 'root',
            },
          ],
        })
      ),
    });

    for (const variant of ['ROOT', 'Root', 'rOoT']) {
      const ctx = createMockCommandContext({ kiro: kiro as any });
      await handleTangent(TAN_CMD, variant, ctx as any);
      expect(kiro.loadSession).toHaveBeenCalled();
      expect(kiro.fork).not.toHaveBeenCalled();
    }
  });

  it('does NOT create a tangent named root', async () => {
    const kiro = mockKiro();
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'root', ctx as any);

    expect(kiro.fork).not.toHaveBeenCalled();
  });

  it('no-ops with alert when already on root', async () => {
    const kiro = mockKiro({
      sessionId: 'root',
      listSessions: mock(() =>
        Promise.resolve({
          sessions: [
            { sessionId: 'root', title: 'Main', parentSessionId: undefined },
            { sessionId: 'child', title: 'child', parentSessionId: 'root' },
          ],
        })
      ),
    });
    const ctx = createMockCommandContext({ kiro: kiro as any });

    await handleTangent(TAN_CMD, 'root', ctx as any);

    expect(kiro.loadSession).not.toHaveBeenCalled();
    expect(kiro.fork).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert).toHaveBeenCalledTimes(1);
    expect((ctx._spies.showAlert as any).mock.calls[0][0]).toContain(
      'Already on root'
    );
  });
});
