/**
 * Tangent tree utilities — builds a visual tree from a flat session list
 * using parentSessionId links.
 */

export interface TangentNode {
  sessionId: string;
  title: string;
  parentSessionId?: string;
  updatedAt?: string;
  children: TangentNode[];
}

export interface FlattenedTangentRow {
  sessionId: string;
  title: string;
  label: string; // Includes tree connector glyphs
  isCurrent: boolean;
}

/**
 * Build a tree from a flat session list by walking parentSessionId links.
 * Returns the root node (session with no parent that is an ancestor of currentSessionId).
 */
export function buildTangentTree(
  sessions: Array<{
    sessionId: string;
    title?: string;
    parentSessionId?: string;
    updatedAt?: string;
  }>,
  currentSessionId: string | undefined
): TangentNode | null {
  // Build lookup maps
  const byId = new Map<string, TangentNode>();
  for (const s of sessions) {
    byId.set(s.sessionId, {
      sessionId: s.sessionId,
      title: s.title || s.sessionId.slice(0, 8),
      parentSessionId: s.parentSessionId,
      updatedAt: s.updatedAt,
      children: [],
    });
  }

  // Link children to parents
  const roots: TangentNode[] = [];
  for (const node of byId.values()) {
    if (node.parentSessionId && byId.has(node.parentSessionId)) {
      byId.get(node.parentSessionId)!.children.push(node);
    } else {
      roots.push(node);
    }
  }

  // Find the root that contains currentSessionId (walk up from current)
  let current = currentSessionId ? byId.get(currentSessionId) : undefined;
  // A provided-but-unresolved anchor means the current session isn't in this
  // listing (e.g. a failed/empty list, or a stale id). Return null so callers
  // abort rather than silently operating on an unrelated tree via roots[0].
  if (currentSessionId && !current) return null;
  if (!current) return roots[0] ?? null;

  // Cycle guard: corrupt parent metadata could form a loop (A -> B -> A) and
  // hang this walk. Cap iterations, mirroring jumpToRoot's walkLimit.
  let walkLimit = 100;
  while (
    walkLimit-- > 0 &&
    current.parentSessionId &&
    byId.has(current.parentSessionId)
  ) {
    current = byId.get(current.parentSessionId)!;
  }

  // Label the root node as "root" for display
  current.title = 'root';

  return current;
}

/**
 * Flatten a tree into rows with connector glyphs for display in Explorer.
 */
export function flattenTreeToRows(
  root: TangentNode,
  currentSessionId: string | undefined
): FlattenedTangentRow[] {
  const rows: FlattenedTangentRow[] = [];

  function walk(
    node: TangentNode,
    prefix: string,
    isLast: boolean,
    isRoot: boolean
  ) {
    const connector = isRoot ? '' : isLast ? '└─ ' : '├─ ';
    const label = prefix + connector + node.title;
    rows.push({
      sessionId: node.sessionId,
      title: node.title,
      label,
      isCurrent: node.sessionId === currentSessionId,
    });

    const childPrefix = isRoot ? '' : prefix + (isLast ? '   ' : '│  ');
    for (let i = 0; i < node.children.length; i++) {
      walk(
        node.children[i]!,
        childPrefix,
        i === node.children.length - 1,
        false
      );
    }
  }

  walk(root, '', true, true);
  return rows;
}

/**
 * Find the parent session ID for the current session.
 * Returns null if current session is the root (no parent).
 */
export function findParentSessionId(
  sessions: Array<{ sessionId: string; parentSessionId?: string }>,
  currentSessionId: string | undefined
): string | null {
  const current = sessions.find((s) => s.sessionId === currentSessionId);
  return current?.parentSessionId ?? null;
}

/**
 * Find a session by title (case-insensitive) within the tangent tree.
 */
export function findSessionByTitle(
  sessions: Array<{
    sessionId: string;
    title?: string;
    parentSessionId?: string;
  }>,
  title: string,
  currentSessionId: string | undefined
): { sessionId: string; title: string } | null {
  // Build the tree to scope search to the current tree only
  const root = buildTangentTree(sessions, currentSessionId);
  if (!root) return null;

  const lowerTitle = title.toLowerCase();
  function search(node: TangentNode): TangentNode | null {
    if (node.title.toLowerCase() === lowerTitle) return node;
    for (const child of node.children) {
      const found = search(child);
      if (found) return found;
    }
    return null;
  }

  const found = search(root);
  return found ? { sessionId: found.sessionId, title: found.title } : null;
}
