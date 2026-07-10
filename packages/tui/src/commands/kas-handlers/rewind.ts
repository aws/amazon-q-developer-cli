import { extractRpcErrorMessage } from '../../utils/error-handling';
import { truncateToRecentTurns } from '../../utils/replay-history';
import { logger } from '../../utils/logger';
import { MessageRole } from '../../stores/app-store';
import { buildPreview } from '../../utils/rewind-preview';
import type { AgentStreamEvent } from '../../types/agent-events';
import type { CommandContext } from '../types';
import type { KasCommand } from '../../kas-commands';
import type { DispatchOptions } from '../dispatcher';

/**
 * KAS-side `/rewind` handler.
 *
 * Builds the turn list directly from the TUI's message store (no backend
 * round-trip for the picker). Calls session/fork on KAS for the actual fork.
 */
export async function handleRewind(
  _cmd: KasCommand,
  args: string,
  ctx: CommandContext,
  _options?: DispatchOptions
): Promise<void> {
  const turnIndex = args.trim();

  if (!turnIndex) {
    const turns = buildTurnList(ctx);
    if (turns.length === 0) {
      ctx.showAlert('No previous turns to rewind to', 'warning', 3000);
      return;
    }
    ctx.setShowRewindExplorer(true, turns);
    return;
  }

  // Fork at selected turn
  const idx = parseInt(turnIndex, 10);
  const turns = buildTurnList(ctx);
  const turn = turns.find((t) => t.logIndex === idx);
  if (!turn?.messageId) {
    ctx.showAlert(`Turn index ${turnIndex} out of range`, 'error', 3000);
    return;
  }

  try {
    const response = await ctx.kiro.executeCommand({
      command: 'rewind',
      args: { turnIndex, messageId: turn.messageId },
    } as any);

    const data = response.data as
      | { sessionId?: string; switchSession?: boolean }
      | undefined;
    if (!response.success || !data?.sessionId) {
      ctx.showAlert(
        extractRpcErrorMessage(response.message, 'Rewind failed'),
        'error',
        5000
      );
      return;
    }

    await loadRewoundSession(ctx, data.sessionId);
  } catch (err) {
    logger.error('[rewind] fork failed', { err });
    ctx.showAlert(extractRpcErrorMessage(err, 'Rewind failed'), 'error', 5000);
  }
}

// --- Turn list builder ---

interface RewindTurn {
  logIndex: number;
  label: string;
  group: string;
  responseSnippet: string;
  messageId?: string;
}

function buildTurnList(ctx: CommandContext): RewindTurn[] {
  const messages = ctx.getMessages() as Array<{
    id: string;
    role: MessageRole;
    content: string;
    name?: string;
    contextPercent?: number;
    kasMessageId?: string;
  }>;

  const turns: RewindTurn[] = [];
  let turnStart = -1;

  for (let i = 0; i < messages.length; i++) {
    if (messages[i]!.role === MessageRole.User) {
      if (turnStart >= 0) {
        turns.push(buildTurn(messages, turnStart, i, turns.length));
      }
      turnStart = i;
    }
  }
  if (turnStart >= 0) {
    turns.push(buildTurn(messages, turnStart, messages.length, turns.length));
  }

  return turns.reverse();
}

function buildTurn(
  messages: Array<{
    id: string;
    role: MessageRole;
    content: string;
    name?: string;
    contextPercent?: number;
    kasMessageId?: string;
  }>,
  start: number,
  end: number,
  index: number
): RewindTurn {
  const userMsg = messages[start]!;
  const label = userMsg.content.split('\n')[0]?.slice(0, 80) || '(empty)';
  const aiMessages = messages.slice(start + 1, end);
  const group =
    userMsg.contextPercent != null
      ? `${Math.round(userMsg.contextPercent)}%`
      : '--';

  return {
    logIndex: index,
    label,
    group,
    responseSnippet: buildPreview(aiMessages),
    messageId: userMsg.kasMessageId ?? userMsg.id,
  };
}

// --- Session loader ---

async function loadRewoundSession(
  ctx: CommandContext,
  sessionId: string
): Promise<void> {
  ctx.clearUIState();
  ctx.resetMessages();
  ctx.setLoadingMessage('Loading rewound session...');

  const buffered: AgentStreamEvent[] = [];
  const restoreKasSession = ctx.beginKasSession('resumed');
  try {
    const session = await ctx.kiro.loadSession(sessionId, (e) =>
      buffered.push(e)
    );

    if (buffered.length > 0) {
      const { events, omittedTurns } = truncateToRecentTurns(buffered, 10);
      if (omittedTurns > 0) {
        ctx.addSystemMessage(
          `⋯ ${omittedTurns} earlier turn${omittedTurns === 1 ? '' : 's'} not shown`,
          true
        );
      }
      const handler = ctx.createStreamEventHandler();
      for (const e of events) handler(e);
      (handler as any).flush?.();
    }

    ctx.setLoadingMessage(null);
    ctx.setSessionId(sessionId);
    if (session.currentModel) ctx.setCurrentModel(session.currentModel);
    if (session.currentAgent)
      ctx.setCurrentAgent(session.currentAgent, { suppressWelcome: true });
  } catch (err) {
    restoreKasSession();
    logger.error('[rewind] loadSession failed', { sessionId, err });
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to load rewound session'),
      'error',
      5000
    );
  }
}
