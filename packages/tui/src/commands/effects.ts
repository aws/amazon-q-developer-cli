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
import type { AgentStreamEvent } from '../types/agent-events.js';
import type {
  KnowledgeEntry,
  McpServerInfo,
  SlashCommand,
  ToolInfo,
} from '../stores/app-store.js';
import { executeShellEscapeTTY } from '../utils/shell-escape.js';
import { writeFileSync, readFileSync, unlinkSync, mkdtempSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';

/** Effect handler function. Returns true if it handled its own messaging. */
type EffectHandler = (
  result: CommandResult | null,
  ctx: CommandContext,
  cmd: SlashCommand,
  args: string
) => boolean | void;

/** Extract command name from TuiCommand union type */
type CommandName = TuiCommand['command'];

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
  | 'loadSession';

/**
 * Command → Effect mapping.
 */
const commandEffects: Partial<Record<string, EffectName>> = {
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

  updateAgent: (result, ctx) => {
    const data = result?.data as { agent?: { name: string } } | undefined;
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
      ctx.setShowHelpPanel(true, data.commands);
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
    ctx.setShowToolsPanel(true, data?.tools ?? []);
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
    const tempDir = mkdtempSync(join(tmpdir(), 'kiro-editor-'));
    const tempFile = join(tempDir, 'prompt.md');

    try {
      writeFileSync(tempFile, args || '');
      const editor = process.env.VISUAL || process.env.EDITOR || 'vi';
      const quotedPath = `'${tempFile.replace(/'/g, "'\\''")}'`;
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

      const content = readFileSync(tempFile, 'utf-8').trim();
      if (!content) {
        ctx.showAlert(
          'Empty content from editor, not submitting.',
          'error',
          3000
        );
        return true;
      }

      ctx.sendMessage(content);
      return true;
    } catch (err) {
      ctx.showAlert(
        err instanceof Error ? err.message : 'Failed to open editor',
        'error',
        3000
      );
      return true;
    } finally {
      try {
        unlinkSync(tempFile);
      } catch {
        // ignore cleanup errors
      }
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
        // Replay buffered history into the message store
        if (buffered.length > 0) {
          const handler = ctx.createStreamEventHandler();
          for (const e of buffered) handler(e);
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
