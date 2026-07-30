import { logger } from '../../utils/logger';
import { extractRpcErrorMessage } from '../../utils/error-handling';
import { switchToKasSession } from './session-switch';
import { resolveTangentSelection } from '../../utils/tangent-nav';
import {
  buildTangentTree,
  flattenTreeToRows,
  findParentSessionId,
  findSessionByTitle,
} from '../../utils/tangent-tree';
import type { CommandContext } from '../types';
import { MessageRole } from '../../types/message-role';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

// Reserved names that cannot be used as tangent names
const RESERVED_NAMES = new Set(['ls', 'root']);

type SessionEntry = {
  sessionId: string;
  title?: string;
  parentSessionId?: string;
  updatedAt?: string;
  description?: string;
};

/**
 * KAS-side `/tangent` handler.
 *
 * - No args: go back to parent (if on tangent) or auto-create (if on root)
 * - `ls`: show interactive tangent tree picker
 * - `<name>`: switch to existing tangent or create a new one
 */
export async function handleTangent(
  _cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  const arg = args.trim();

  if (!arg) {
    return goBackOrAutoCreate(ctx);
  }

  if (arg.toLowerCase() === 'ls') {
    return showTree(ctx);
  }

  if (arg.toLowerCase() === 'root') {
    return jumpToRoot(ctx);
  }

  return switchOrCreate(ctx, arg);
}

// --- Go back or auto-create ---

async function goBackOrAutoCreate(ctx: CommandContext): Promise<void> {
  const sessions = await listSessions(ctx);
  if (!sessions) return;

  const parentId = findParentSessionId(sessions, ctx.kiro.sessionId);
  if (parentId) {
    return goBack(ctx, sessions);
  }

  // On root — auto-create a new tangent with a generated name. Number against
  // the ENTIRE tree using the same lookup switchOrCreate uses
  // (findSessionByTitle is whole-tree), so the generated tangent-N is
  // guaranteed free across the whole tree. This keeps tangent names unique:
  // bare /tangent always creates a NEW tangent instead of colliding with a
  // tangent-N deeper in the tree and silently switching into it.
  let counter = 1;
  while (
    findSessionByTitle(sessions, `tangent-${counter}`, ctx.kiro.sessionId)
  ) {
    counter++;
  }
  return switchOrCreate(ctx, `tangent-${counter}`, sessions);
}

// --- Go back to parent ---

async function goBack(
  ctx: CommandContext,
  sessions: SessionEntry[]
): Promise<void> {
  const parentId = findParentSessionId(sessions, ctx.kiro.sessionId);
  if (!parentId) {
    ctx.showAlert(
      'Already on root session (no parent to go back to)',
      'warning',
      3000
    );
    return;
  }

  const parentSession = sessions.find((s) => s.sessionId === parentId);
  const isRoot = !parentSession?.parentSessionId;
  const label = isRoot ? 'root' : parentSession?.title || 'parent';

  await switchToKasSession(ctx, parentId, {
    loadingLabel: `Switching to ${label}...`,
    // Derive tangent state from the session list (same source as `label`), not
    // the load response — the load response's parentSessionId is not reliably
    // populated across KAS versions, which would wrongly clear the chip when
    // returning to a mid-tree tangent (e.g. root → ddb → ddb1, back to ddb).
    resolveTangentName: () => (isRoot ? null : parentSession?.title || null),
    successAlert: `Returned to ${label}`,
    errorLabel: 'Failed to switch session',
    logTag: 'tangent',
  });
}

// --- Jump to root ---

async function jumpToRoot(ctx: CommandContext): Promise<void> {
  const sessions = await listSessions(ctx);
  if (!sessions) return;

  // Walk up parentSessionId from current session to find the root (no parent)
  const startId = ctx.kiro.sessionId;
  if (!startId) {
    ctx.showAlert('No active session', 'warning', 3000);
    return;
  }
  let currentId: string = startId;
  let walkLimit = 100;
  while (walkLimit-- > 0) {
    const session = sessions.find((s) => s.sessionId === currentId);
    if (!session?.parentSessionId) break;
    currentId = session.parentSessionId;
  }

  const rootId: string = currentId;

  // Already on root — no-op
  if (rootId === ctx.kiro.sessionId) {
    ctx.showAlert('Already on root session', 'warning', 3000);
    return;
  }

  await switchToKasSession(ctx, rootId, {
    loadingLabel: 'Switching to root...',
    resolveTangentName: () => null,
    successAlert: 'Returned to root',
    errorLabel: 'Failed to switch',
    logTag: 'tangent',
  });
}

// --- Show tree ---

async function showTree(ctx: CommandContext): Promise<void> {
  const sessions = await listSessions(ctx);
  if (!sessions) return;

  const root = buildTangentTree(sessions, ctx.kiro.sessionId);
  if (!root) {
    ctx.showAlert('No tangent tree found', 'warning', 3000);
    return;
  }

  const flatRows = flattenTreeToRows(root, ctx.kiro.sessionId);
  if (flatRows.length <= 1) {
    ctx.showAlert(
      'No tangents yet. Use /tangent <name> to create one.',
      'warning',
      3000
    );
    return;
  }

  const sessionMap = new Map(sessions.map((s) => [s.sessionId, s]));

  ctx.setShowTangentExplorer(
    true,
    flatRows.map((row) => {
      const s = sessionMap.get(row.sessionId);
      return {
        id: row.sessionId,
        label: row.label,
        title: row.title,
        isCurrent: row.isCurrent,
        isTangent: !!s?.parentSessionId,
        lastActive: s?.updatedAt ? relativeTime(s.updatedAt) : '',
      };
    })
  );
}

// --- Switch to or create tangent ---

async function switchOrCreate(
  ctx: CommandContext,
  name: string,
  prefetchedSessions?: SessionEntry[]
): Promise<void> {
  if (RESERVED_NAMES.has(name.toLowerCase())) {
    ctx.showAlert(
      `"${name}" is a reserved name and cannot be used as a tangent name`,
      'error',
      3000
    );
    return;
  }

  const sessions = prefetchedSessions ?? (await listSessions(ctx));
  if (!sessions) return;

  // Explorer selection dispatches the target session id directly (see
  // useBackendPanelHandlers.handleTangentSelect), so switch to that exact
  // session — root, sibling, or descendant — rather than resolving by title.
  // Guarded by the reserved-name check above; real session ids never collide
  // with reserved names. Selecting the current session is a no-op.
  const byId = sessions.find((s) => s.sessionId === name);
  if (byId) {
    const decision = resolveTangentSelection(name, ctx.kiro.sessionId);
    if (decision.action === 'noop') return;
    const isRoot = !byId.parentSessionId;
    const label = isRoot ? 'root' : byId.title || 'tangent';
    await switchToKasSession(ctx, byId.sessionId, {
      loadingLabel: `Switching to ${label}...`,
      resolveTangentName: () => (isRoot ? null : byId.title || null),
      successAlert: `Switched to ${label}`,
      errorLabel: 'Failed to switch',
      logTag: 'tangent',
    });
    return;
  }

  // Try to find existing tangent by name
  const existing = findSessionByTitle(sessions, name, ctx.kiro.sessionId);
  if (existing) {
    await switchToKasSession(ctx, existing.sessionId, {
      loadingLabel: `Switching to "${existing.title}"...`,
      resolveTangentName: (session) => session.title || existing.title,
      successAlert: `Switched to "${existing.title}"`,
      errorLabel: 'Failed to switch',
      logTag: 'tangent',
    });
    return;
  }

  // Create new tangent: fork current session
  // KAS cannot fork a session with no messages (session/fork throws
  // NO_FORK_POINT), which surfaces to the user as an opaque "Internal error".
  // Guard with a friendly hint instead.
  if (!ctx.getMessages().some((m) => m.role === MessageRole.User)) {
    ctx.showAlert(
      'Nothing to branch from yet — send a message first, then /tangent',
      'warning',
      4000
    );
    return;
  }
  let newSessionId: string;
  try {
    const response = await ctx.kiro.fork({
      createdReason: 'tangent',
      title: name,
    });

    const data = response.data as { sessionId?: string } | undefined;
    if (!response.success || !data?.sessionId) {
      ctx.showAlert(
        extractRpcErrorMessage(response.message, 'Failed to create tangent'),
        'error',
        5000
      );
      return;
    }
    newSessionId = data.sessionId;
  } catch (err) {
    logger.error('[tangent] create failed', { err });
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to create tangent'),
      'error',
      5000
    );
    return;
  }

  await switchToKasSession(ctx, newSessionId, {
    loadingLabel: `Created tangent "${name}"...`,
    resolveTangentName: (session) => session.title || name,
    successAlert: `On tangent "${name}"`,
    errorLabel: 'Failed to create tangent',
    logTag: 'tangent',
  });
}

// --- Helpers ---

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return 'just now';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

async function listSessions(
  ctx: CommandContext
): Promise<SessionEntry[] | null> {
  try {
    const result = await ctx.kiro.listSessions(process.cwd());
    // KasAcpClient.listSessions swallows RPC errors and returns
    // { sessions: [], failed: true } rather than throwing, so a transient list
    // failure is indistinguishable from a genuinely empty result unless we
    // check the flag. Abort (return null) so callers don't auto-create a new
    // tangent or fork a duplicate against an empty list on a read failure.
    if (result.failed) {
      ctx.showAlert('Failed to list sessions', 'error', 3000);
      return null;
    }
    return result.sessions;
  } catch (err) {
    logger.error('[tangent] listSessions failed', { err });
    ctx.showAlert('Failed to list sessions', 'error', 3000);
    return null;
  }
}
