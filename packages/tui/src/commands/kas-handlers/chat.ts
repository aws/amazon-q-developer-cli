import { extractRpcErrorMessage } from '../../utils/error-handling';
import { formatRelativeTime } from '../../utils/sessions';
import { truncateToRecentTurns } from '../../utils/replay-history';
import { logger } from '../../utils/logger';
import type { AgentStreamEvent } from '../../types/agent-events';
import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

export async function handleChat(
  cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  options?: DispatchOptions
): Promise<void> {
  const trimmed = args.trim();

  if (!trimmed) {
    return showSessionPicker(ctx, cmd);
  }
  if (trimmed === 'save' || trimmed.startsWith('save ')) {
    ctx.showAlert('/chat save is not yet supported', 'error', 3000);
    return;
  }
  if (trimmed === 'load' || trimmed.startsWith('load ')) {
    ctx.showAlert('/chat load is not yet supported', 'error', 3000);
    return;
  }
  if (trimmed === 'new' || trimmed.startsWith('new ')) {
    const prompt = trimmed === 'new' ? null : trimmed.slice(4).trim() || null;
    return startNewSession(ctx, prompt);
  }
  if (options?.argIsSynthetic) {
    return loadExistingSession(ctx, trimmed);
  }
  ctx.showAlert(`Unknown /chat subcommand: ${trimmed}`, 'error', 3000);
}

async function showSessionPicker(
  ctx: CommandContext,
  cmd: KasCommand
): Promise<void> {
  try {
    ctx.setLoadingMessage('Loading chat options...');
    const { sessions } = await ctx.kiro.listSessions(process.cwd());
    ctx.setLoadingMessage(null);
    const currentSessionId = ctx.kiro.sessionId;
    const options = sessions
      .filter((s) => s.sessionId !== currentSessionId)
      .filter((s) => s.title != null)
      .map((s) => ({
        value: s.sessionId,
        label: `${s.title!} (${s.sessionId.slice(0, 8)})`,
        description: s.updatedAt ? formatRelativeTime(s.updatedAt) : undefined,
      }));
    if (options.length === 0) {
      ctx.showAlert('No previous sessions found', 'error', 3000);
      return;
    }
    ctx.setActiveCommand({ command: cmd, options });
  } catch (err) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to list sessions'),
      'error',
      3000
    );
  }
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
  ctx.clearUIState();
  ctx.setLoadingMessage(`Loading session ${sessionId}...`);
  // Buffer history events during load via direct onUpdate subscriber, then
  // replay them after the load resolves so the conversation renders in order.
  const buffered: AgentStreamEvent[] = [];
  try {
    const session = await ctx.kiro.loadSession(sessionId, (e) =>
      buffered.push(e)
    );
    logger.debug('[chat] loadSession resolved', {
      sessionId,
      bufferedCount: buffered.length,
    });
    ctx.addSystemMessage(`Loaded session ${sessionId}`, true);
    if (buffered.length > 0) {
      const MAX_DISPLAY_TURNS = 10;
      const { events, omittedTurns } = truncateToRecentTurns(
        buffered,
        MAX_DISPLAY_TURNS
      );
      if (omittedTurns > 0) {
        ctx.addSystemMessage(
          `⋯ ${omittedTurns} earlier turn${omittedTurns === 1 ? '' : 's'} not shown`,
          true
        );
      }
      const handler = ctx.createStreamEventHandler();
      for (const e of events) handler(e);
      // TODO: extend the createStreamEventHandler return type to expose
      // `flush` rather than escaping through `as any`. V2 has the same cast
      // and both should be cleaned up together.
      (handler as any).flush?.();
    }
    ctx.setLoadingMessage(null);
    ctx.setSessionId(sessionId);
    if (session.currentModel) ctx.setCurrentModel(session.currentModel);
    if (session.currentAgent)
      ctx.setCurrentAgent(session.currentAgent, { suppressWelcome: true });
    ctx.showAlert('Session loaded', 'success', 3000);
  } catch (err) {
    logger.error('[chat] loadSession failed', {
      sessionId,
      err: JSON.stringify(err),
      stack: err instanceof Error ? err.stack : undefined,
    });
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to load session'),
      'error',
      5000
    );
  }
}
