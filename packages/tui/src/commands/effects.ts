/**
 * Effect registry for slash commands.
 *
 * Two tables:
 * 1. commandEffects: maps command name → effect name
 * 2. effectHandlers: maps effect name → handler function
 *
 * Command names derived from TuiCommand type (typeshare generated).
 */

import type { CommandContext } from './types.js';
import type { CommandResult, TuiCommand } from '../types/commands.js';
import { logger } from '../utils/logger.js';
import {
  AgentEventType,
  type AgentStreamEvent,
} from '../types/agent-events.js';
import type {
  KnowledgeEntry,
  McpServerInfo,
  SlashCommand,
  ToolInfo,
} from '../stores/app-store.js';
import { openEditorSync } from '../utils/editor.js';
import { executeShellEscapeTTY } from '../utils/shell-escape.js';
import { readFileSync } from 'fs';

/** Effect handler function. Returns true if it handled its own messaging. */
type EffectHandler = (
  result: CommandResult | null,
  ctx: CommandContext,
  cmd: SlashCommand,
  args: string
) => boolean | void;

/** Extract command name from TuiCommand union type */
type CommandName = TuiCommand['command'];

/**
 * Keep only the last `maxTurns` user turns from a buffered event stream.
 * A "turn" starts at each UserMessage event and includes all subsequent
 * events until the next UserMessage.  Returns the truncated slice and
 * how many turns were dropped.
 */
function truncateToRecentTurns(
  events: AgentStreamEvent[],
  maxTurns: number
): { events: AgentStreamEvent[]; omittedTurns: number } {
  // Find indices where each user turn starts
  const turnStarts: number[] = [];
  for (let i = 0; i < events.length; i++) {
    if (events[i]!.type === AgentEventType.UserMessage) {
      turnStarts.push(i);
    }
  }
  if (turnStarts.length <= maxTurns) {
    return { events, omittedTurns: 0 };
  }
  const keepFrom = turnStarts[turnStarts.length - maxTurns]!;
  return {
    events: events.slice(keepFrom),
    omittedTurns: turnStarts.length - maxTurns,
  };
}

/** Effect names - semantic actions the TUI can perform */
type EffectName =
  | 'updateModel'
  | 'updateAgent'
  | 'showContextPanel'
  | 'showHelpPanel'
  | 'showUsagePanel'
  | 'showMcpPanel'
  | 'showToolsPanel'
  | 'showKnowledgePanel'
  | 'executePrompt'
  | 'clearMessages'
  | 'quit'
  | 'pasteImage'
  | 'promptEditor'
  | 'loadSession'
  | 'replyEditor'
  | 'showCodePanel'
  | 'showFeedbackUrl';

/**
 * Command → Effect mapping.
 */
const commandEffects: Partial<Record<string, EffectName>> = {
  feedback: 'showFeedbackUrl',
  help: 'showHelpPanel',
  model: 'updateModel',
  agent: 'updateAgent',
  plan: 'updateAgent',
  context: 'showContextPanel',
  usage: 'showUsagePanel',
  prompts: 'executePrompt',
  clear: 'clearMessages',
  quit: 'quit',
  mcp: 'showMcpPanel',
  tools: 'showToolsPanel',
  knowledge: 'showKnowledgePanel',
  paste: 'pasteImage',
  editor: 'promptEditor',
  chat: 'loadSession',
  reply: 'replyEditor',
  code: 'showCodePanel',
};

/**
 * Effect handlers.
 */
const effectHandlers: Record<EffectName, EffectHandler> = {
  updateModel: (result, ctx) => {
    const data = result?.data as
      | { model?: { id: string; name: string } }
      | undefined;
    if (data?.model) {
      ctx.setCurrentModel(data.model);
    }
  },

  updateAgent: (result, ctx, _cmd) => {
    const data = result?.data as
      | { agent?: { name: string }; path?: string; name?: string }
      | undefined;

    // If the result contains a path, it's an agent create/edit — open editor then validate
    if (data?.path) {
      const filePath = data.path;
      const editor = process.env.VISUAL || process.env.EDITOR || 'vi';
      const quotedPath = `'${filePath.replace(/'/g, "'\\''")}'`;
      const { exitCode, error } = executeShellEscapeTTY(
        `${editor} ${quotedPath}`
      );

      if (exitCode !== 0) {
        ctx.showAlert(
          error ?? `Editor exited with code ${exitCode}`,
          'error',
          3000
        );
        return true;
      }

      // Post-editor validation: check valid JSON with required "name" field
      try {
        const content = readFileSync(filePath, 'utf-8');
        const parsed = JSON.parse(content);
        if (
          typeof parsed !== 'object' ||
          parsed === null ||
          typeof parsed.name !== 'string' ||
          !parsed.name.trim()
        ) {
          ctx.showAlert(
            `Malformed agent config at ${filePath}: missing or invalid "name" field`,
            'error',
            5000
          );
          return true;
        }
      } catch (e) {
        const msg =
          e instanceof SyntaxError
            ? `Malformed agent config at ${filePath}: ${e.message}`
            : `Failed to read agent config at ${filePath}: ${e}`;
        ctx.showAlert(msg, 'error', 5000);
        return true;
      }

      ctx.showAlert(result?.message ?? 'Done', 'success', 5000);
      return true;
    }

    if (data?.agent) {
      ctx.setCurrentAgent(data.agent);
    }
  },

  showContextPanel: (result, ctx) => {
    // If the result has breakdown data, show the panel (this is /context show or bare /context)
    const data = result?.data as
      | { breakdown?: any; contextUsagePercentage?: number }
      | undefined;
    if (data?.breakdown) {
      if (data?.contextUsagePercentage != null) {
        ctx.setContextUsage(data.contextUsagePercentage);
      }
      ctx.setShowContextBreakdown(true, data?.breakdown);
    }
    // Otherwise it's an add/remove/clear result - alert is shown by dispatcher step 4
  },

  showHelpPanel: (result, ctx) => {
    const data = result?.data as
      | {
          commands?: Array<{
            name: string;
            description: string;
            usage: string;
            subcommands?: string[];
          }>;
        }
      | undefined;
    if (data?.commands) {
      // Merge backend commands with TUI-local commands for complete help listing
      const localHelpEntries = ctx.slashCommands
        .filter((c) => c.source === 'local')
        .map((c) => ({
          name: c.name,
          description: c.description,
          usage: c.name,
        }));
      const allCommands = [...data.commands, ...localHelpEntries].sort((a, b) =>
        a.name.localeCompare(b.name)
      );
      ctx.setShowHelpPanel(true, allCommands);
    }
  },

  showUsagePanel: (result, ctx) => {
    ctx.setShowUsagePanel(true, result?.data);
  },

  showMcpPanel: (result, ctx) => {
    const data = result?.data as { servers?: McpServerInfo[] } | undefined;
    ctx.setShowMcpPanel(true, data?.servers ?? []);
  },

  showToolsPanel: (result, ctx) => {
    const data = result?.data as { tools?: ToolInfo[] } | undefined;
    if (data?.tools) {
      ctx.setShowToolsPanel(true, data.tools);
    }
    // Subcommands (trust-all, reset) return no tools data — let dispatcher show the alert
  },

  showKnowledgePanel: (result, ctx) => {
    const data = result?.data as
      | { entries?: KnowledgeEntry[]; status?: string }
      | undefined;
    if (data?.entries) {
      ctx.setShowKnowledgePanel(true, data.entries, data.status);
    } else {
      ctx.setShowKnowledgePanel(false);
      if (result?.message) {
        const firstLine = result.message.split('\n')[0] ?? result.message;
        ctx.showAlert(firstLine, result.success ? 'success' : 'error');
      }
    }
  },

  executePrompt: (result, ctx) => {
    const data = result?.data as { executePrompt?: string } | undefined;
    if (data?.executePrompt) {
      ctx.sendMessage(data.executePrompt);
    }
  },

  clearMessages: (result, ctx) => {
    ctx.clearMessages();
  },

  quit: (_result, ctx) => {
    ctx.kiro.close();
    process.exit(0);
  },

  /** Open $EDITOR to compose a prompt, then send the content as a chat message */
  promptEditor: (_result, ctx, _cmd, args) => {
    const result = openEditorSync({
      prefix: 'kiro-editor-',
      filename: 'prompt.md',
      initialContent: args || '',
      validate: (c) =>
        !c ? 'Empty content from editor, not submitting.' : undefined,
    });
    if (!result.ok) {
      ctx.showAlert(result.error, 'error', 3000);
      return true;
    }
    ctx.sendMessage(result.content);
    return true;
  },

  /** Open $EDITOR pre-filled with the last assistant message (quoted) to compose a reply */
  replyEditor: (result, ctx) => {
    if (!result?.success) {
      ctx.showAlert(
        result?.message ?? 'No assistant message found',
        'error',
        3000
      );
      return true;
    }
    const data = result?.data as { initialContent?: string } | undefined;
    const initialContent = data?.initialContent ?? '';
    const editorResult = openEditorSync({
      prefix: 'kiro-reply-',
      filename: 'reply.md',
      initialContent,
      validate: (c) =>
        !c || c === initialContent.trim()
          ? 'No changes made in editor, not submitting.'
          : undefined,
    });
    if (!editorResult.ok) {
      ctx.showAlert(editorResult.error, 'error', 3000);
      return true;
    }
    ctx.sendMessage(editorResult.content);
    return true;
  },

  showCodePanel: (result, ctx) => {
    const data = result?.data as Record<string, unknown> | undefined;
    if (data?.executePrompt) {
      const prompt = data.executePrompt as string;
      const label = data.label as string | undefined;
      ctx.sendMessage(prompt, undefined, label);
      return true;
    }
    if (data) {
      ctx.setShowCodePanel(true, data as any);
    } else {
      ctx.setShowCodePanel(false);
      if (result?.message) {
        ctx.showAlert(result.message, result.success ? 'success' : 'error');
      }
    }
  },

  showFeedbackUrl: (result, ctx) => {
    const data = result?.data as { url?: string } | undefined;
    if (data?.url) {
      // URL is in the message from backend; use longer timeout so user can copy it
      ctx.showAlert(result?.message ?? data.url, 'error', 10000);
      return true;
    }
  },

  pasteImage: (result, ctx) => {
    const data = result?.data as
      | {
          data?: string;
          mimeType?: string;
          width?: number;
          height?: number;
          sizeBytes?: number;
        }
      | undefined;
    if (data?.data && data.mimeType) {
      ctx.sendMessage(formatImageLabel(data), [
        { base64: data.data, mimeType: data.mimeType },
      ]);
    } else if (result?.message && !result.success) {
      ctx.showAlert(result.message, 'error');
    }
  },

  loadSession: (_result, ctx, _cmd, args) => {
    if (!args) return;
    const sessionId = args;
    ctx.clearUIState();
    ctx.setLoadingMessage(`Loading session ${sessionId}...`);

    // Buffer history events during load via direct onUpdate subscriber
    const buffered: AgentStreamEvent[] = [];

    ctx.kiro
      .loadSession(sessionId, (e) => buffered.push(e))
      .then((session) => {
        logger.debug('[chat] loadSession resolved', {
          sessionId,
          bufferedCount: buffered.length,
        });
        // Add a visual delimiter before replaying history
        ctx.addSystemMessage(`Loaded session ${sessionId}`, true);
        // Replay buffered history into the message store, capped to recent turns
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
          (handler as any).flush?.();
        }
        ctx.setLoadingMessage(null);
        ctx.setSessionId(sessionId);
        if (session.currentModel) ctx.setCurrentModel(session.currentModel);
        if (session.currentAgent) ctx.setCurrentAgent(session.currentAgent);
        ctx.showAlert('Session loaded', 'success', 3000);
      })
      .catch((err: unknown) => {
        logger.error('[chat] loadSession failed', {
          sessionId,
          err: JSON.stringify(err),
          stack: err instanceof Error ? err.stack : undefined,
        });
        ctx.setLoadingMessage(null);
        const data =
          typeof err === 'object' && err !== null && 'data' in err
            ? String((err as any).data)
            : undefined;
        const message =
          data ??
          (err instanceof Error ? err.message : 'Failed to load session');
        ctx.showAlert(message, 'error', 5000);
      });
  },
};

import { formatImageLabel } from '../utils/image-label.js';

/**
 * Run effect for a command.
 * Returns true if the effect handled its own messaging (suppresses dispatcher step 4).
 */
export function runEffect(
  cmd: SlashCommand,
  result: CommandResult | null,
  ctx: CommandContext,
  args: string
): boolean {
  const cmdName = cmd.name.replace(/^\//, '');
  const effectName = commandEffects[cmdName as CommandName];
  if (effectName) {
    return effectHandlers[effectName]?.(result, ctx, cmd, args) === true;
  }
  return false;
}
