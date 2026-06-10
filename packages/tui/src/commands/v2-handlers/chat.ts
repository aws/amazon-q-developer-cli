/**
 * V2-engine handler for `/chat`. Owns the entire command flow when
 * the active engine is V2 (mirrors `kas-handlers/chat.ts` for the
 * KAS engine).
 *
 * Subcommands:
 *  - (no args)         : open a session picker over the merged
 *                        V1+V2+KAS listing and route the selection
 *                        through ensure-session before loading.
 *  - `<sessionId>`     : load an existing session by id. Bare ids
 *                        for non-V2 sources auto-convert via
 *                        `ensure-session --source auto`.
 *  - `save [--force] <path>` : delegate to the V2 backend
 *                              (`agent::tui_commands::chat::execute`).
 *  - `load <path>`           : delegate to the V2 backend, then
 *                              load the imported session.
 *  - `new [prompt]`          : start a fresh session via
 *                              `ctx.kiro.newSession()`.
 */

import { extractRpcErrorMessage } from '../../utils/error-handling';
import { listAllSessions } from '../../utils/list-all-sessions-cli';
import { ensureSession } from '../../utils/ensure-session-cli';
import {
  isResumableSource,
  isActiveEngineSource,
} from '../../utils/cross-engine-session-id';
import { formatRelativeTime } from '../../utils/sessions';
import { sanitizeSessionTitleForDisplay } from '../../utils/sanitize-title';
import { runSessionLoad } from '../session-load';
import { logger } from '../../utils/logger';
import type { TuiCommand, AvailableCommand } from '../../types/commands';
import type { CommandContext } from '../types';
import type { DispatchOptions } from '../dispatcher';

export async function handleChat(
  cmd: AvailableCommand,
  args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  const trimmed = args.trim();

  if (!trimmed) {
    return showSessionPicker(ctx, cmd);
  }
  if (trimmed === 'save' || trimmed.startsWith('save ')) {
    return handleSave(ctx, trimmed);
  }
  if (trimmed === 'load' || trimmed.startsWith('load ')) {
    return handleLoadFromPath(ctx, trimmed);
  }
  if (trimmed === 'new' || trimmed.startsWith('new ')) {
    const prompt = trimmed === 'new' ? null : trimmed.slice(4).trim() || null;
    return startNewSession(ctx, prompt);
  }
  // Bare sessionId: either typed by the user (`/chat <id>`) or routed
  // back via the picker selection (`argIsSynthetic`).
  return loadExistingSession(ctx, trimmed);
}

async function showSessionPicker(
  ctx: CommandContext,
  cmd: AvailableCommand
): Promise<void> {
  ctx.setLoadingMessage('Loading chat options...');
  const listing = await listAllSessions();
  ctx.setLoadingMessage(null);
  if (!listing.ok) {
    ctx.showAlert(`Failed to list sessions: ${listing.error}`, 'error', 3000);
    return;
  }
  const currentSessionId = ctx.kiro.sessionId;
  // Active engine is always V2 here: this handler runs only when the
  // dispatcher's KAS intercept hasn't fired.
  const activeIsKas = false;
  const options = listing.sessions
    .filter((s) => s.sessionId !== currentSessionId)
    .filter((s) => isResumableSource(s.source, activeIsKas))
    .map((s) => {
      const native = isActiveEngineSource(s.source, activeIsKas);
      const sourceTag = native ? '' : ` (${s.source})`;
      return {
        value: s.sessionId,
        label: `${sanitizeSessionTitleForDisplay(s.title)} (${s.sessionId.slice(0, 8)})${sourceTag}`,
        description: formatRelativeTime(s.updatedAt),
      };
    });
  if (options.length === 0) {
    ctx.showAlert('No previous sessions found', 'error', 3000);
    return;
  }
  ctx.setActiveCommand({ command: cmd, options });
}

async function startNewSession(
  ctx: CommandContext,
  prompt: string | null
): Promise<void> {
  ctx.clearUIState();
  ctx.resetMessages();
  ctx.setLoadingMessage('Starting new conversation...');
  try {
    const session = await ctx.kiro.newSession();
    ctx.setLoadingMessage(null);
    ctx.setSessionId(session.sessionId);
    if (session.currentModel) ctx.setCurrentModel(session.currentModel);
    if (session.currentAgent) ctx.setCurrentAgent(session.currentAgent);
    ctx.showAlert(
      'New conversation started. Use /chat to switch back.',
      'success',
      3000
    );
    if (prompt) ctx.sendMessage(prompt);
  } catch (err) {
    logger.error('[chat] newSession failed', {
      err: JSON.stringify(err),
      stack: err instanceof Error ? err.stack : undefined,
    });
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to start new conversation'),
      'error',
      5000
    );
  }
}

async function loadExistingSession(
  ctx: CommandContext,
  sessionId: string
): Promise<void> {
  // Always route through ensure-session with `auto` source: native
  // ids resolve via a fast filesystem probe; non-native ids
  // (e.g. picking a KAS session in V2 mode) trigger conversion.
  ctx.setLoadingMessage(`Resolving session ${sessionId}...`);
  const ensured = await ensureSession({
    sourceFormat: 'auto',
    sourceSessionId: sessionId,
    targetFormat: 'v2',
    cwd: process.cwd(),
  });
  ctx.setLoadingMessage(null);
  if (!ensured.ok) {
    ctx.showAlert(`Failed to load session: ${ensured.error}`, 'error', 5000);
    return;
  }
  runSessionLoad(ensured.sessionId, ctx, {
    resetMessagesBeforeReplay: false,
    suppressAgentWelcome: false,
  });
}

async function handleSave(ctx: CommandContext, args: string): Promise<void> {
  // Delegate to the V2 backend; the ACP server implements the export
  // (`crates/chat-cli-v2/src/agent/acp/commands/chat.rs::save_session`).
  try {
    const result = await ctx.kiro.executeCommand({
      command: 'chat',
      args: { value: args },
    } as TuiCommand);
    ctx.showAlert(result.message, result.success ? 'success' : 'error', 5000);
  } catch (err) {
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to save session'),
      'error',
      5000
    );
  }
}

async function handleLoadFromPath(
  ctx: CommandContext,
  args: string
): Promise<void> {
  // Delegate to the V2 backend's import-session implementation, then
  // hand off the returned `sessionId` to the standard load flow.
  let result;
  try {
    result = await ctx.kiro.executeCommand({
      command: 'chat',
      args: { value: args },
    } as TuiCommand);
  } catch (err) {
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to load session'),
      'error',
      5000
    );
    return;
  }
  const data = result?.data as { sessionId?: string } | undefined;
  if (!result?.success || !data?.sessionId) {
    ctx.showAlert(result?.message ?? 'Failed to load session', 'error', 5000);
    return;
  }
  runSessionLoad(data.sessionId, ctx, {
    resetMessagesBeforeReplay: false,
    suppressAgentWelcome: false,
  });
}
