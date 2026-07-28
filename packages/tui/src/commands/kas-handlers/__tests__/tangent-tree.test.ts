import { describe, it, expect } from 'bun:test';
import {
  buildTangentTree,
  flattenTreeToRows,
  findParentSessionId,
  findSessionByTitle,
} from '../../../utils/tangent-tree';

describe('tangent-tree', () => {
  const sessions = [
    { sessionId: 'root', title: 'Main', parentSessionId: undefined },
    { sessionId: 'child1', title: 'experiment-auth', parentSessionId: 'root' },
    { sessionId: 'child2', title: 'refactor-parser', parentSessionId: 'root' },
    {
      sessionId: 'grandchild',
      title: 'auth-v2-retry',
      parentSessionId: 'child1',
    },
  ];

  describe('buildTangentTree', () => {
    it('builds tree from flat list with root identified from current session', () => {
      const tree = buildTangentTree(sessions, 'child2');
      expect(tree).not.toBeNull();
      expect(tree!.sessionId).toBe('root');
      expect(tree!.children).toHaveLength(2);
    });

    it('finds root when current session is deeply nested', () => {
      const tree = buildTangentTree(sessions, 'grandchild');
      expect(tree!.sessionId).toBe('root');
    });

    it('returns null for empty session list', () => {
      const tree = buildTangentTree([], 'anything');
      expect(tree).toBeNull();
    });

    it('handles single session (root only)', () => {
      const tree = buildTangentTree(
        [{ sessionId: 'solo', title: 'Solo' }],
        'solo'
      );
      expect(tree!.sessionId).toBe('solo');
      expect(tree!.children).toHaveLength(0);
    });

    it('returns null when the anchor is absent (no unrelated-tree fallback)', () => {
      // A non-empty list containing only an UNRELATED tree; the anchor is not
      // in it. Must return null rather than falling back to roots[0] (which
      // would silently operate on a stranger's conversation tree).
      const unrelated = [
        { sessionId: 'otherRoot', title: 'Other', parentSessionId: undefined },
        { sessionId: 'otherChild', title: 'x', parentSessionId: 'otherRoot' },
      ];
      expect(buildTangentTree(unrelated, 'not-in-list')).toBeNull();
    });

    it('terminates on cyclic parent metadata (cycle guard)', () => {
      // a <-> b parent cycle: without the walk cap this would loop forever and
      // hang the TUI. The test completing at all proves the guard terminates.
      const cyclic = [
        { sessionId: 'a', title: 'A', parentSessionId: 'b' },
        { sessionId: 'b', title: 'B', parentSessionId: 'a' },
      ];
      expect(buildTangentTree(cyclic, 'a')).not.toBeNull();
    });
  });

  describe('flattenTreeToRows', () => {
    it('produces rows with tree connectors', () => {
      const tree = buildTangentTree(sessions, 'root')!;
      const rows = flattenTreeToRows(tree, 'child2');

      expect(rows).toHaveLength(4);
      expect(rows[0]!.label).toBe('root');
      expect(rows[0]!.isCurrent).toBe(false);

      // Children should have connectors
      const child1Row = rows.find((r) => r.sessionId === 'child1');
      expect(child1Row!.label).toContain('├─');
      expect(child1Row!.label).toContain('experiment-auth');

      const child2Row = rows.find((r) => r.sessionId === 'child2');
      expect(child2Row!.label).toContain('└─');
      expect(child2Row!.isCurrent).toBe(true);
    });

    it('handles deep nesting with correct indentation', () => {
      const tree = buildTangentTree(sessions, 'root')!;
      const rows = flattenTreeToRows(tree, 'grandchild');

      const grandchildRow = rows.find((r) => r.sessionId === 'grandchild');
      expect(grandchildRow!.label).toContain('└─');
      expect(grandchildRow!.label).toContain('auth-v2-retry');
      expect(grandchildRow!.isCurrent).toBe(true);
    });
  });

  describe('findParentSessionId', () => {
    it('returns parent ID for child session', () => {
      expect(findParentSessionId(sessions, 'child1')).toBe('root');
    });

    it('returns null for root session', () => {
      expect(findParentSessionId(sessions, 'root')).toBeNull();
    });

    it('returns null for unknown session', () => {
      expect(findParentSessionId(sessions, 'unknown')).toBeNull();
    });
  });

  describe('findSessionByTitle', () => {
    it('finds session by exact title (case insensitive)', () => {
      const found = findSessionByTitle(sessions, 'Experiment-Auth', 'root');
      expect(found).not.toBeNull();
      expect(found!.sessionId).toBe('child1');
    });

    it('finds nested session by title', () => {
      const found = findSessionByTitle(sessions, 'auth-v2-retry', 'root');
      expect(found).not.toBeNull();
      expect(found!.sessionId).toBe('grandchild');
    });

    it('returns null when title not found', () => {
      const found = findSessionByTitle(sessions, 'nonexistent', 'root');
      expect(found).toBeNull();
    });
  });
});
