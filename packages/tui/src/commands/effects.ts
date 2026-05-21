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
import { type AgentStreamEvent } from '../types/agent-events.js';
import {
  truncateToRecentTurns,
  MAX_DISPLAY_TURNS,
} from '../utils/truncate-history.js';
import type {
  HookInfo,
  KnowledgeEntry,
  McpServerInfo,
  SlashCommand,
  ToolInfo,
} from '../stores/app-store.js';
import type { AvailableCommand } from '../types/commands.js';
import { openEditorSync } from '../utils/editor.js';
import { executeShellEscapeTTY } from '../utils/shell-escape.js';
import { extractRpcErrorMessage } from '../utils/error-handling.js';
import { Kiro } from '../kiro.js';
import {
  describeSpecDocuments,
  findSpecFeature,
  listSpecFeatures,
  type SpecFeatureSummary,
} from '../utils/spec-workspace.js';
import {
  resolveArtifactPath,
  type ArtifactKind,
} from '../utils/spec-artifact-loader.js';
import { readFileSync, writeFileSync, statSync } from 'fs';

import { openTranscriptInPager } from '../utils/open-transcript.js';
import {
  findSettingsSubcommand,
  buildSettingsActiveCommand,
} from './settings-subcommands.js';

/** Effect handler function. Returns true if it handled its own messaging. */
export type EffectHandler = (
  result: CommandResult | null,
  ctx: CommandContext,
  cmd: AvailableCommand,
  args: string
) => boolean | void | Promise<boolean | void>;

/** Extract command name from TuiCommand union type */
type CommandName = TuiCommand['command'] | 'spawn' | 'spec';

/** Effect names - semantic actions the TUI can perform */
type EffectName =
  | 'updateModel'
  | 'updateAgent'
  | 'showContextPanel'
  | 'showHelpPanel'
  | 'showUsagePanel'
  | 'showMcpPanel'
  | 'showToolsPanel'
  | 'showHooksPanel'
  | 'showKnowledgePanel'
  | 'executePrompt'
  | 'clearMessages'
  | 'quit'
  | 'pasteImage'
  | 'promptEditor'
  | 'newSession'
  | 'loadSession'
  | 'replyEditor'
  | 'showCodePanel'
  | 'showFeedbackUrl'
  | 'spawnSession'
  | 'runSpec'
  | 'switchSession'
  | 'copyToClipboard'
  | 'openRawView'
  | 'showThemeMenu'
  | 'showSettingsMenu'
  | 'showTuiPanel'
  | 'showChangelogPanel'
  | 'showSessionId'
  | 'showStatsPanel'
  | 'switchToGuideAgent'
  | 'rewindAction';

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
  exit: 'quit',
  mcp: 'showMcpPanel',
  tools: 'showToolsPanel',
  stats: 'showStatsPanel',
  hooks: 'showHooksPanel',
  knowledge: 'showKnowledgePanel',
  paste: 'pasteImage',
  editor: 'promptEditor',
  chat: 'loadSession',
  reply: 'replyEditor',
  code: 'showCodePanel',
  spawn: 'spawnSession',
  spec: 'runSpec',
  copy: 'copyToClipboard',
  transcript: 'openRawView',
  theme: 'showThemeMenu',
  settings: 'showSettingsMenu',
  tui: 'showTuiPanel',
  changelog: 'showChangelogPanel',
  'session-id': 'showSessionId',
  guide: 'switchToGuideAgent',
  rewind: 'rewindAction',
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
      | {
          breakdown?: any;
          contextUsagePercentage?: number;
          initialExpanded?: boolean;
        }
      | undefined;
    if (data?.breakdown) {
      if (data?.contextUsagePercentage != null) {
        ctx.setContextUsage(data.contextUsagePercentage);
      }
      ctx.setShowContextBreakdown(true, {
        ...data.breakdown,
        initialExpanded: data.initialExpanded,
      });
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
    const data = result?.data as
      | {
          servers?: McpServerInfo[];
          registryServers?: McpServerInfo[];
          message?: string;
          mode?: string;
        }
      | undefined;
    ctx.setShowMcpPanel(
      true,
      data?.servers ?? [],
      data?.mode ?? 'list',
      data?.registryServers
    );
  },

  showToolsPanel: (result, ctx) => {
    const data = result?.data as { tools?: ToolInfo[] } | undefined;
    if (data?.tools) {
      ctx.setShowToolsPanel(true, data.tools);
    }
    // Subcommands (trust-all, reset) return no tools data — let dispatcher show the alert
  },

  showStatsPanel: (result, ctx, _cmd, args) => {
    if (args?.startsWith('save')) {
      ctx.setActiveCommand(null);
      ctx.showAlert(
        result?.message ?? 'Done',
        result?.success ? 'success' : 'error',
        3000
      );
      return true;
    }
    const data = result?.data as { stats?: any[]; summary?: any } | undefined;
    ctx.setShowStatsPanel(true, data?.stats ?? [], data?.summary ?? null);
  },

  showHooksPanel: (result, ctx) => {
    const data = result?.data as { hooks?: HookInfo[] } | undefined;
    ctx.setShowHooksPanel(true, data?.hooks ?? []);
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
    // KAS mode: /clear composes session/new + UI reset, so the result carries
    // a new sessionId.  Do a full wipe and adopt the new session.
    // Rust mode: backend cleared history on the existing session, so keep the
    // last turn visible (legacy behavior).
    const data = result?.data as
      | {
          sessionId?: string;
          currentModel?: { id: string; name: string };
          currentAgent?: { name: string; welcomeMessage?: string };
        }
      | undefined;
    if (data?.sessionId) {
      ctx.clearUIState();
      ctx.resetMessages();
      ctx.setSessionId(data.sessionId);
      if (data.currentModel) ctx.setCurrentModel(data.currentModel);
      if (data.currentAgent) ctx.setCurrentAgent(data.currentAgent);
      return;
    }
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
    let initialContent = data?.initialContent ?? '';

    // KAS mode fallback: compute from buffered messages.
    // Collects all Model messages after the last User message (handles
    // multi-part responses where tool calls interleave text).
    if (!initialContent) {
      const messages = ctx.getMessages();
      const parts: string[] = [];
      for (let i = messages.length - 1; i >= 0; i--) {
        const msg = messages[i]!;
        if (msg.role === MessageRole.User) break;
        if (msg.role === MessageRole.Model && msg.content) {
          parts.push(msg.content);
        }
      }
      const lastContent = parts.reverse().join('\n\n');
      if (lastContent) {
        initialContent =
          lastContent
            .split('\n')
            .map((line) => `> ${line}`)
            .join('\n') + '\n\n';
      }
    }

    // Handle case where no assistant message exists
    if (!initialContent) {
      ctx.showAlert('No assistant message found', 'error', 3000);
      return true;
    }

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
      ctx.showAlert(result?.message ?? data.url, 'warning', 10000);
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

  newSession: (_result, ctx, _cmd, args) => {
    const prompt = args === 'new' ? null : args.slice(4).trim() || null;
    ctx.clearUIState();
    ctx.resetMessages();
    ctx.setLoadingMessage('Starting new conversation...');
    ctx.kiro
      .newSession()
      .then((session) => {
        logger.debug('[chat] newSession resolved', {
          sessionId: session.sessionId,
        });
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
      })
      .catch((err: unknown) => {
        logger.error('[chat] newSession failed', {
          err: JSON.stringify(err),
          stack: err instanceof Error ? err.stack : undefined,
        });
        ctx.setLoadingMessage(null);
        const message = extractRpcErrorMessage(
          err,
          'Failed to start new conversation'
        );
        ctx.showAlert(message, 'error', 5000);
      });
    return true;
  },

  loadSession: (_result, ctx, _cmd, args) => {
    // /rewind <idx> — backend cloned the session, now auto-load the new one.
    // Detect via the switchSession flag so we don't collide with /chat semantics.
    const resultData = _result?.data as
      | {
          sessionId?: string;
          switchSession?: boolean;
          suppressAgentWelcome?: boolean;
          resetMessagesBeforeReplay?: boolean;
        }
      | undefined;
    const suppressAgentWelcome = resultData?.suppressAgentWelcome === true;
    const resetMessagesBeforeReplay =
      resultData?.resetMessagesBeforeReplay === true;
    if (resultData?.switchSession && resultData.sessionId) {
      if (!_result?.success) {
        if (_result?.message) ctx.showAlert(_result.message, 'error', 5000);
        return true;
      }
      args = resultData.sessionId;
    }

    if (!args) return;

    // /chat save — show result and done
    if (/^save\b/.test(args)) {
      if (_result?.message) {
        ctx.showAlert(
          _result.message,
          _result.success ? 'success' : 'error',
          5000
        );
      }
      return true;
    }

    // /chat load <path> — backend imported the file, now auto-load the new session
    if (/^load\b/.test(args)) {
      const data = _result?.data as { sessionId?: string } | undefined;
      if (!_result?.success || !data?.sessionId) {
        if (_result?.message) {
          ctx.showAlert(_result.message, 'error', 5000);
        }
        return true;
      }
      // Fall through to the session-load logic below with the imported session ID
      args = data.sessionId;
    }

    // /chat <sessionId> — load an existing session
    const sessionId = args;
    ctx.clearUIState();
    if (resetMessagesBeforeReplay) {
      // /rewind-only: drop the previous session's live messages so the
      // forked session's replayed history doesn't stack on top of them
      // (and more importantly, so stale turns from the old session don't
      // appear to be part of the forked session's context).
      ctx.resetMessages();
    }
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
        if (session.currentAgent)
          ctx.setCurrentAgent(session.currentAgent, {
            suppressWelcome: suppressAgentWelcome,
          });
        ctx.showAlert('Session loaded', 'success', 3000);
      })
      .catch((err: unknown) => {
        logger.error('[chat] loadSession failed', {
          sessionId,
          err: JSON.stringify(err),
          stack: err instanceof Error ? err.stack : undefined,
        });
        ctx.setLoadingMessage(null);
        // Previously did `String((err as any).data)` which produces
        // "[object Object]" when `data` is structured (e.g. KAS auth errors).
        // Use the shared extractor that understands ACP's RequestError shape.
        const message = extractRpcErrorMessage(err, 'Failed to load session');
        ctx.showAlert(message, 'error', 5000);
      });
  },

  switchSession: (_result, ctx, _cmd, args) => {
    const sessions = Array.from(ctx.sessions.values()).filter(
      (s) => s.status !== 'pending'
    );

    if (sessions.length === 0) {
      ctx.showAlert('No active sessions', 'error', 3000);
      return;
    }

    // If arg provided, switch directly by name or id prefix
    if (args) {
      if (args === '' || args === 'main') {
        ctx.setActiveSession('');
        ctx.showAlert('Switched to main chat', 'success', 2000);
        return;
      }
      const target = sessions.find(
        (s) => s.name === args || s.id.startsWith(args)
      );
      if (target) {
        ctx.setActiveSession(target.id);
        process.stdout.write('\x1b[?1049h'); // enter alt screen
        ctx.setMode('session-view');
        ctx.showAlert(`Switched to ${target.name}`, 'success', 2000);
      } else {
        ctx.showAlert(`Session not found: ${args}`, 'error', 3000);
      }
      return;
    }

    // No arg — show selection menu
    const switchCmd = ctx.slashCommands.find((c) => c.name === '/switch');
    if (!switchCmd) return;

    ctx.setActiveCommand({
      command: switchCmd,
      options: [
        {
          value: 'main',
          label: 'main chat',
          description: 'return to main conversation',
        },
        ...sessions.map((s) => ({
          value: s.id,
          label: s.name,
          description: `${s.status}${s.role ? ` · ${s.role}` : ''}${s.group ? ` · ${s.group}` : ''}`,
        })),
      ],
    });
  },

  spawnSession: async (result, ctx, _cmd, args) => {
    if (!args) {
      ctx.showAlert('Task description is required', 'error', 3000);
      return;
    }

    try {
      // Parse args for --name flag
      const parts = args.split(/\s+/);
      let task = '';
      let name: string | undefined;

      for (let i = 0; i < parts.length; i++) {
        if (parts[i] === '--name' && i + 1 < parts.length) {
          name = parts[i + 1];
          i++; // Skip the name value
        } else {
          task += (task ? ' ' : '') + parts[i];
        }
      }

      if (!task) {
        ctx.showAlert('Task description is required', 'error', 3000);
        return;
      }

      // Spawn the session
      const { sessionId, name: assignedName } = await ctx.kiro.spawnSession(
        task,
        name
      );
      const displayName =
        assignedName || name || `session-${sessionId.slice(0, 8)}`;

      // Create session object for store
      const session = {
        id: sessionId,
        name: displayName,
        role: undefined,
        group: undefined,
        status: 'idle' as const,
        type: 'ephemeral' as const,
        created: new Date(),
        lastActivity: new Date(),
        summary: undefined,
        parentSession: undefined,
      };

      // Add to store
      ctx.addSession(session);

      ctx.showAlert(
        `Spawned ${displayName}: ${task.slice(0, 40)}${task.length > 40 ? '…' : ''}`,
        'success',
        3000
      );
    } catch (error) {
      const message = extractRpcErrorMessage(error, 'Failed to spawn session');
      ctx.showAlert(message, 'error', 3000);
    }
  },

  /**
   * /spec — list feature directories under `.kiro/specs/` and drive the KAS
   * spec workflow.
   *
   * Subcommands (all KAS-only; surface a clear error when the agent doesn't
   * advertise `_kiro/spec/*`):
   *   /spec                    → selection menu of discovered specs
   *   /spec <name>             → switch to spec mode and resume work on <name>
   *   /spec new <name>         → switch to spec mode and ask the agent to
   *                              start a fresh spec for <name>
   *   /spec run <name>         → invoke `_kiro/spec/invoke runAllTasks` and
   *                              let the agent drive to completion
   */
  runSpec: async (_result, ctx, cmd, args) => {
    const trimmed = args.trim();
    const workspaceRoot = process.cwd();

    // No-args path: delegate to the shared `openSpecView` helper
    // which handles both the empty-args feature picker and the
    // `<name> [artifact]` direct-open path.
    if (!trimmed) {
      return openSpecView(ctx, cmd, workspaceRoot, '');
    }

    // /spec new <name> — switch to spec mode, then nudge the agent to
    // kick off the spec workflow via a normal prompt. The agent's spec
    // mode knows how to create the feature directory and the initial
    // requirements document when asked to start a new spec.
    if (/^new(\s|$)/.test(trimmed)) {
      const name = trimmed.slice(3).trim();
      if (!name) {
        ctx.showAlert('Usage: /spec new <feature-name>', 'error', 4000);
        return true;
      }
      try {
        await ctx.kiro.setMode('spec');
      } catch (err) {
        ctx.showAlert(
          extractRpcErrorMessage(err, 'Failed to switch to spec mode'),
          'error',
          5000
        );
        return true;
      }
      ctx.setCurrentAgent({ name: 'spec' });
      await ctx.sendMessage(
        `Start a new spec called "${name}". Create the .kiro/specs/${name}/ directory and draft the initial requirements document.`
      );
      return true;
    }

    // /spec run <name> — invoke runAllTasks via ACP ext method. The agent
    // drives the execution from there; we surface the outcome via a toast
    // since there's no dedicated progress UI yet.
    if (/^run(\s|$)/.test(trimmed)) {
      const name = trimmed.slice(3).trim();
      if (!name) {
        ctx.showAlert('Usage: /spec run <feature-name>', 'error', 4000);
        return true;
      }
      const feature = findSpecFeature(workspaceRoot, name);
      if (!feature) {
        ctx.showAlert(`No spec found at .kiro/specs/${name}/`, 'error', 5000);
        return true;
      }
      if (!feature.tasksFilePath) {
        ctx.showAlert(
          `Spec "${name}" has no tasks.md yet — generate it first.`,
          'error',
          5000
        );
        return true;
      }
      await runSpecFeature(ctx, feature);
      return true;
    }

    // /spec view <name> [requirements|design|tasks] — explicit alias for
    // the default action below. Kept so existing muscle memory and the
    // tab-completion flow that surfaces `view` as a subcommand still work.
    if (/^view(\s|$)/.test(trimmed)) {
      const rest = trimmed.slice(4).trim();
      // Strip the `view` prefix and fall through to the default-view
      // handler with the remainder (which may be empty, in which case
      // the picker fires; or `<name> [artifact]`, in which case we
      // open the panel).
      return openSpecView(ctx, cmd, workspaceRoot, rest);
    }

    // /spec <name> — default action: open the structured view panel for
    // the named feature. The "continue work on this spec" path that
    // used to live here is now triggered by pressing `c` inside the
    // view panel (see useArtifactKeybinds).
    return openSpecView(ctx, cmd, workspaceRoot, trimmed);
  },

  /** Copy last assistant response to system clipboard */
  copyToClipboard: async (_result, ctx) => {
    const messages = ctx.getMessages();
    // Collect all Model messages from the last assistant turn (everything
    // after the most recent User message). Tool calls interleave Model
    // messages, so we need to concatenate all of them.
    const parts: string[] = [];
    for (let i = messages.length - 1; i >= 0; i--) {
      const msg = messages[i]!;
      if (msg.role === MessageRole.User) break;
      if (msg.role === MessageRole.Model && msg.content) {
        parts.push(msg.content);
      }
    }
    const lastContent = parts.reverse().join('\n\n');

    if (!lastContent) {
      ctx.showAlert('No response to copy', 'error', 3000);
      return true;
    }

    if (!copyToSystemClipboard(lastContent)) {
      ctx.showAlert('Failed to copy — no clipboard tool found', 'error', 3000);
      return true;
    }

    ctx.showAlert('Copied to clipboard', 'success', 3000);
    return true;
  },

  /** Open full conversation as raw markdown in $PAGER */
  openRawView: (_result, ctx) => {
    const messages = ctx.getMessages();
    if (!messages.length) {
      ctx.showAlert('No conversation to display', 'error', 3000);
      return true;
    }

    openTranscriptInPager(messages);
    return true;
  },

  /** Show theme color selection menu */
  showTuiPanel: (_result, ctx) => {
    ctx.setShowTuiPanel(true);
  },

  showChangelogPanel: (_result, ctx) => {
    ctx.setShowChangelogPanel(true);
  },

  showSessionId: (_result, ctx) => {
    const sessionId = ctx.kiro.sessionId ?? 'none';
    ctx.showAlert(
      sessionId !== 'none'
        ? `Session ID: ${sessionId}\nResume with: kiro-cli --resume ${sessionId}`
        : 'Session ID: none',
      'success',
      10000
    );
    return true;
  },

  showThemeMenu: (_result, ctx, cmd, args) => {
    // Deprecation notice shown once per menu-open. Skipped on in-menu
    // selections (args !== '') to avoid re-nagging, and on calls chained
    // from /settings theme (cmd.name !== '/theme').
    if (cmd.name === '/theme' && args === '') {
      ctx.showAlert('/theme has moved to /settings theme', 'warning', 4000);
    }

    const prefs = loadUserThemePrefs();
    const themeCmd = ctx.slashCommands.find((c) => c.name === '/theme');
    if (!themeCmd) return;

    const fallbackDiff = buildFallbackDiff(ctx.getThemeDiffHex());

    // /theme bundled:default — reset to auto-detected theme
    if (args === 'bundled:default') {
      ctx.setUserColors(null, null, null);
      ctx.setBaseTheme(null);
      const saved = saveUserThemePrefs({});
      ctx.showAlert(
        saved ? 'Theme reset to default' : 'Theme reset but failed to save',
        saved ? 'success' : 'error',
        3000
      );
      ctx.setThemePreview(null);
      return true;
    }

    // /theme bundled:<id> — apply a bundled theme (Light/Dark)
    if (args.startsWith('bundled:')) {
      const themeId = args.slice('bundled:'.length);
      const bundled = getBundledTheme(themeId);
      if (!bundled) {
        ctx.showAlert(`Unknown theme: ${themeId}`, 'error', 3000);
        return true;
      }
      // Switch the base theme (kiroDark/kiroLight) so ALL UI elements update
      ctx.setBaseTheme(
        themeId === 'light' ? kiroLight : themeId === 'dark' ? kiroDark : null
      );
      ctx.setUserColors(
        { text: bundled.prompt.textColor, bg: bundled.prompt.bgColor },
        bundled.response.textColor,
        bundled.diff
      );
      const baseThemePref: 'dark' | 'light' | undefined =
        themeId === 'light' ? 'light' : themeId === 'dark' ? 'dark' : undefined;
      const saved = saveUserThemePrefs({
        promptPreset:
          bundled.prompt.id === 'default' ? undefined : bundled.prompt.id,
        responsePreset:
          bundled.response.id === 'default' ? undefined : bundled.response.id,
        diffPreset: bundled.diff.id === 'default' ? undefined : bundled.diff.id,
        baseTheme: baseThemePref,
      });
      ctx.showAlert(
        saved
          ? `Theme set to ${bundled.label}`
          : `Theme applied but failed to save`,
        saved ? 'success' : 'error',
        3000
      );
      ctx.setThemePreview(null);
      return true;
    }

    // /theme custom — show prompt vs response vs diff selection with current preview
    if (args === 'custom') {
      const currentPrompt = getPromptPreset(prefs.promptPreset);
      const currentResponse = getResponsePreset(prefs.responsePreset);
      const currentDiff = getDiffPreset(prefs.diffPreset);
      ctx.setThemePreview(buildCurrentPreview(prefs, fallbackDiff));
      ctx.setActiveCommand({
        command: {
          ...themeCmd,
          meta: {
            ...themeCmd.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options: [
          {
            value: 'prompt',
            label: 'Prompt style',
            description: currentPrompt ? currentPrompt.label : 'Default',
          },
          {
            value: 'response',
            label: 'Response text color',
            description: currentResponse ? currentResponse.label : 'Default',
          },
          {
            value: 'diff',
            label: 'Code diff colors',
            description: currentDiff ? currentDiff.label : 'Default',
          },
        ],
      });
      return true;
    }

    // /theme prompt — show prompt combo presets with preview
    if (args === 'prompt') {
      ctx.setThemePreview(buildCurrentPreview(prefs, fallbackDiff));
      ctx.setActiveCommand({
        command: {
          ...themeCmd,
          meta: {
            ...themeCmd.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options: promptPresets.map((p) => {
          const isActive = p.id === (prefs.promptPreset ?? 'default');
          return {
            value: `prompt:${p.id}`,
            label: p.label,
            description: isActive ? '[active]' : '',
          };
        }),
      });
      return true;
    }

    // /theme response — show response color presets with preview
    if (args === 'response') {
      ctx.setThemePreview(buildCurrentPreview(prefs, fallbackDiff));
      ctx.setActiveCommand({
        command: {
          ...themeCmd,
          meta: {
            ...themeCmd.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options: responsePresets.map((p) => {
          const isActive = p.id === (prefs.responsePreset ?? 'default');
          return {
            value: `response:${p.id}`,
            label: p.label,
            description: isActive ? '[active]' : '',
          };
        }),
      });
      return true;
    }

    // /theme diff — show diff color presets with preview
    if (args === 'diff') {
      ctx.setThemePreview(buildCurrentPreview(prefs, fallbackDiff));
      ctx.setActiveCommand({
        command: {
          ...themeCmd,
          meta: {
            ...themeCmd.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options: diffPresets.map((p) => {
          const isActive = p.id === (prefs.diffPreset ?? 'default');
          return {
            value: `diff:${p.id}`,
            label: p.label,
            description: isActive ? '[active]' : '',
          };
        }),
      });
      return true;
    }

    // /theme prompt:<id>, response:<id>, or diff:<id> — apply custom selection, then return to custom menu
    if (
      args.startsWith('prompt:') ||
      args.startsWith('response:') ||
      args.startsWith('diff:')
    ) {
      const colonIdx = args.indexOf(':');
      const category = args.slice(0, colonIdx);
      const presetId = args.slice(colonIdx + 1);

      const updatedPrefs = { ...prefs };
      if (category === 'prompt') {
        const preset = getPromptPreset(presetId);
        if (!preset) {
          ctx.showAlert(`Unknown prompt preset: ${presetId}`, 'error', 3000);
          return true;
        }
        updatedPrefs.promptPreset =
          preset.id === 'default' ? undefined : preset.id;
        ctx.setUserColors(
          { text: preset.textColor, bg: preset.bgColor },
          undefined,
          undefined
        );
        const saved1 = saveUserThemePrefs(updatedPrefs);
        ctx.showAlert(
          saved1
            ? `Prompt style set to ${preset.label}`
            : `Prompt applied but failed to save`,
          saved1 ? 'success' : 'error',
          3000
        );
      } else if (category === 'response') {
        const preset = getResponsePreset(presetId);
        if (!preset) {
          ctx.showAlert(`Unknown response preset: ${presetId}`, 'error', 3000);
          return true;
        }
        updatedPrefs.responsePreset =
          preset.id === 'default' ? undefined : preset.id;
        ctx.setUserColors(undefined, preset.textColor, undefined);
        const saved2 = saveUserThemePrefs(updatedPrefs);
        ctx.showAlert(
          saved2
            ? `Response color set to ${preset.label}`
            : `Response applied but failed to save`,
          saved2 ? 'success' : 'error',
          3000
        );
      } else {
        // diff
        const preset = getDiffPreset(presetId);
        if (!preset) {
          ctx.showAlert(`Unknown diff preset: ${presetId}`, 'error', 3000);
          return true;
        }
        updatedPrefs.diffPreset =
          preset.id === 'default' ? undefined : preset.id;
        ctx.setUserColors(undefined, undefined, preset);
        const saved3 = saveUserThemePrefs(updatedPrefs);
        ctx.showAlert(
          saved3
            ? `Diff colors set to ${preset.label}`
            : `Diff applied but failed to save`,
          saved3 ? 'success' : 'error',
          3000
        );
      }

      // Return to custom menu with updated preview
      const currentPrompt = getPromptPreset(updatedPrefs.promptPreset);
      const currentResponse = getResponsePreset(updatedPrefs.responsePreset);
      const currentDiff = getDiffPreset(updatedPrefs.diffPreset);
      ctx.setThemePreview(buildCurrentPreview(updatedPrefs, fallbackDiff));
      ctx.setActiveCommand({
        command: {
          ...themeCmd,
          meta: {
            ...themeCmd.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options: [
          {
            value: 'prompt',
            label: 'Prompt style',
            description: currentPrompt ? currentPrompt.label : 'Default',
          },
          {
            value: 'response',
            label: 'Response text color',
            description: currentResponse ? currentResponse.label : 'Default',
          },
          {
            value: 'diff',
            label: 'Code diff colors',
            description: currentDiff ? currentDiff.label : 'Default',
          },
        ],
      });
      return true;
    }

    // Bare /theme — show top-level: Auto, Dark Theme, Light Theme, Custom
    // Initial preview matches first highlighted item (Auto)
    ctx.setThemePreview(ctx.getAutoPreview() || null);

    // Determine which option is currently active
    const activeBundledId = bundledThemes.find((t) => {
      const matchPrompt = (prefs.promptPreset ?? 'default') === t.prompt.id;
      const matchResponse =
        (prefs.responsePreset ?? 'default') === t.response.id;
      const matchDiff = (prefs.diffPreset ?? 'default') === t.diff.id;
      return matchPrompt && matchResponse && matchDiff;
    })?.id;
    const isCustomActive =
      !activeBundledId &&
      (prefs.promptPreset || prefs.responsePreset || prefs.diffPreset);

    const isDefaultActive = !activeBundledId && !isCustomActive;

    ctx.setActiveCommand({
      command: {
        ...themeCmd,
        meta: {
          ...themeCmd.meta,
          inputType: 'selection' as const,
          searchable: false,
        },
      },
      options: [
        {
          value: 'bundled:default',
          label: 'Auto',
          description: isDefaultActive
            ? '[active]'
            : 'Auto-detected theme for your terminal',
        },
        ...bundledThemes.map((t) => ({
          value: `bundled:${t.id}`,
          label: t.label,
          description: t.id === activeBundledId ? '[active]' : '',
        })),
        {
          value: 'custom',
          label: 'Custom',
          description: isCustomActive
            ? '[active]'
            : 'Choose prompt, response, and diff colors separately',
        },
      ],
    });
    return true;
  },

  /**
   * Open the /settings menu or route to a specific subcommand.
   *
   * Subcommands and their routing logic live in ./settings-subcommands.ts.
   * To add a new one, add an entry there — no changes needed here.
   */
  showSettingsMenu: (_result, ctx, cmd, args) => {
    if (args) {
      const sub = findSettingsSubcommand(args);
      if (!sub) {
        ctx.showAlert(`Unknown settings subcommand: ${args}`, 'error', 3000);
        return true;
      }
      void Promise.resolve(
        sub.handle({
          ctx,
          settingsCommand: cmd,
          resolveEffect: (name) => {
            const handler = effectHandlers[name as EffectName];
            if (!handler) {
              throw new Error(`Unknown effect handler: ${name}`);
            }
            return handler;
          },
        })
      );
      return true;
    }

    ctx.setActiveCommand(buildSettingsActiveCommand(cmd));
    return true;
  },

  switchToGuideAgent: (result, ctx) => {
    const data = result?.data as
      | { agent?: { name: string }; prompt?: string }
      | undefined;
    if (data?.agent) {
      ctx.setCurrentAgent(data.agent);
    }
    if (data?.prompt) {
      ctx.sendMessage(data.prompt);
    }
  },

  rewindAction: (result, ctx, cmd, args) => {
    // /rewind <idx> — backend cloned the session, now auto-load the new one.
    const data = result?.data as
      | {
          sessionId?: string;
          switchSession?: boolean;
          turns?: Array<{
            logIndex: number;
            label: string;
            group: string;
            responseSnippet: string;
          }>;
        }
      | undefined;
    if (data?.switchSession && data.sessionId) {
      if (!result?.success) {
        if (result?.message) ctx.showAlert(result.message, 'error', 5000);
        return true;
      }
      // Defer to the loadSession handler to actually switch the TUI over.
      const loadSessionHandler = effectHandlers.loadSession;
      if (loadSessionHandler) {
        return loadSessionHandler(
          {
            success: true,
            message: '',
            data: {
              sessionId: data.sessionId,
              switchSession: true,
              // /rewind-only: skip the agent welcome message on reload since
              // the user is continuing, not starting fresh.
              suppressAgentWelcome: true,
              // /rewind-only: clear live messages before replaying the forked
              // session's history so stale turns from the old session don't
              // leak into the new session's display.
              resetMessagesBeforeReplay: true,
            },
          },
          ctx,
          cmd,
          args
        );
      }
      return true;
    }

    // /rewind with no args — backend returned the turn list under
    // `result.data.turns`. Open the Explorer with those rows.
    const turns = data?.turns ?? [];
    if (turns.length === 0) {
      ctx.showAlert('No previous turns to rewind to', 'warning', 3000);
      return true;
    }
    ctx.setShowRewindExplorer(true, turns);
    return true;
  },
};

import { formatImageLabel } from '../utils/image-label.js';
import { MessageRole } from '../stores/app-store.js';
import { kiroDark } from '../theme/kiroDark.js';
import { kiroLight } from '../theme/kiroLight.js';
import {
  promptPresets,
  responsePresets,
  diffPresets,
  bundledThemes,
  buildCurrentPreview,
  buildFallbackDiff,
  loadUserThemePrefs,
  saveUserThemePrefs,
  getPromptPreset,
  getResponsePreset,
  getDiffPreset,
  getBundledTheme,
} from '../theme/user-theme.js';
import { spawnSync } from 'child_process';

/**
 * Pick the most-recently-modified artifact among requirements/design/tasks
 * under `.kiro/specs/<feature>/`. Returns null when none of the three
 * artifact files exist.
 *
 * Used for `/spec view <feature>` (no explicit artifact arg) so the user
 * lands on the most-active document by default.
 */
function pickMostRecentArtifact(
  workspaceRoot: string,
  featureName: string
): ArtifactKind | null {
  const candidates: ArtifactKind[] = ['requirements', 'design', 'tasks'];
  let best: { kind: ArtifactKind; mtime: number } | null = null;
  for (const kind of candidates) {
    const path = resolveArtifactPath(workspaceRoot, featureName, kind);
    try {
      const s = statSync(path);
      // Use mtimeMs so we can compare with simple > and don't lose
      // sub-second precision (POSIX mtime in seconds is too coarse for
      // generation events that arrive within the same second).
      const mtime = s.mtimeMs;
      if (!best || mtime > best.mtime) {
        best = { kind, mtime };
      }
    } catch {
      // File missing or unreadable: skip it.
    }
  }
  return best?.kind ?? null;
}

/**
 * Render the structured view panel for a spec.
 *
 * Shared between `/spec <name>` (default) and `/spec view <name>`. When
 * `rest` is empty, surfaces a feature picker; otherwise parses
 * `<name> [artifact]` and opens the panel directly.
 *
 * Returns `true` (the standard "effect handled the message" signal) in
 * all cases — even when an alert is shown for an invalid feature name.
 */
async function openSpecView(
  ctx: CommandContext,
  cmd: AvailableCommand,
  workspaceRoot: string,
  rest: string
): Promise<boolean> {
  if (!rest) {
    const features = listSpecFeatures(workspaceRoot);
    if (features.length === 0) {
      ctx.showAlert(
        'No specs found under .kiro/specs/. Use "/spec new <name>" to start one.',
        'warning',
        6000
      );
      return true;
    }
    ctx.setActiveCommand({
      command: {
        ...cmd,
        meta: { ...cmd.meta, inputType: 'selection' as const },
      },
      options: features.map((f) => ({
        // Re-enter as `/spec view <name>` so picker selection routes
        // straight into this same handler.
        value: `view ${f.featureName}`,
        label: f.featureName,
        description: describeSpecDocuments(f),
      })),
    });
    return true;
  }

  const parts = rest.split(/\s+/);
  const name = parts[0]!;
  const explicitArtifact = parts[1];

  const feature = findSpecFeature(workspaceRoot, name);
  if (!feature) {
    ctx.showAlert(`No spec found at .kiro/specs/${name}/`, 'error', 5000);
    return true;
  }

  let artifact: ArtifactKind;
  if (explicitArtifact !== undefined) {
    if (
      explicitArtifact !== 'requirements' &&
      explicitArtifact !== 'design' &&
      explicitArtifact !== 'tasks'
    ) {
      ctx.showAlert(
        `Unknown artifact "${explicitArtifact}". Use one of: requirements, design, tasks.`,
        'error',
        5000
      );
      return true;
    }
    artifact = explicitArtifact;
  } else {
    const picked = pickMostRecentArtifact(workspaceRoot, name);
    if (!picked) {
      ctx.showAlert(
        `No artifact files in .kiro/specs/${name}/ — generate requirements/design/tasks first.`,
        'error',
        5000
      );
      return true;
    }
    artifact = picked;
  }

  await ctx.openArtifactView(name, artifact);
  return true;
}

/**
 * Resolve a spec session and invoke `runAllTasks` via the KAS ACP ext
 * methods.  The agent drives execution autonomously from there — the TUI
 * observes progress through the normal session-update stream.
 */
async function runSpecFeature(
  ctx: CommandContext,
  feature: SpecFeatureSummary
): Promise<void> {
  try {
    ctx.setLoadingMessage(`Running all tasks for ${feature.featureName}...`);
    const { sessionId } = await ctx.kiro.resolveSpecSession({
      featureName: feature.featureName,
      strategy: 'reuse',
    });
    await ctx.kiro.invokeSpec({
      operation: 'runAllTasks',
      sessionId,
      featureName: feature.featureName,
      specDocuments: feature.specDocumentPaths,
      tasksFilePath: feature.tasksFilePath!,
    });
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      `Running all tasks for "${feature.featureName}" — the agent is working autonomously.`,
      'success',
      5000
    );
  } catch (err) {
    ctx.setLoadingMessage(null);
    ctx.showAlert(
      extractRpcErrorMessage(err, 'Failed to run spec tasks'),
      'error',
      5000
    );
  }
}

/**
 * Switch to spec mode and ask the agent to continue work on a feature.
 *
 * Extracted as a free function so the artifact-view `c` keybind can
 * reuse the exact same path the slash command used to take. Keeping
 * the wording stable matters: KAS's spec workflow keys off the prompt
 * shape ("Continue working on the …") to know it's resuming an
 * existing feature rather than starting a new one.
 *
 * Returns nothing; surfaces failures via `showAlert` so the caller
 * doesn't have to handle errors.
 */
export interface ResumeSpecDeps {
  kiro: Kiro;
  setCurrentAgent: (agent: { name: string } | null) => void;
  sendMessage: (content: string) => Promise<void> | void;
  showAlert: (
    message: string,
    status: 'error' | 'success' | 'warning',
    autoHideMs?: number
  ) => void;
}

export async function resumeSpecFeature(
  deps: ResumeSpecDeps,
  feature: SpecFeatureSummary
): Promise<void> {
  try {
    await deps.kiro.setMode('spec');
  } catch (err) {
    deps.showAlert(
      extractRpcErrorMessage(err, 'Failed to switch to spec mode'),
      'error',
      5000
    );
    return;
  }
  deps.setCurrentAgent({ name: 'spec' });
  await deps.sendMessage(
    `Continue working on the "${feature.featureName}" spec. The existing documents are: ${feature.documents.join(', ')}.`
  );
}
/**
 * Copy text to the system clipboard using platform-native tools.
 * Returns true if a clipboard tool was found and executed without error.
 *
 * Strategy per platform:
 *   macOS  → pbcopy (always available)
 *   Windows → powershell Set-Clipboard (handles UTF-8 correctly, unlike clip.exe)
 *   Linux  → wl-copy (Wayland) → xclip (X11) → xsel (X11 fallback)
 */
export function copyToSystemClipboard(text: string): boolean {
  const candidates: Array<{ bin: string; args: string[] }> = [];

  if (process.platform === 'darwin') {
    candidates.push({ bin: 'pbcopy', args: [] });
  } else if (process.platform === 'win32') {
    // powershell's Set-Clipboard handles UTF-8; clip.exe expects UTF-16
    candidates.push({
      bin: 'powershell',
      args: ['-NoProfile', '-Command', 'Set-Clipboard -Value $input'],
    });
  } else {
    // Linux: try Wayland first, then X11 tools
    if (process.env.WAYLAND_DISPLAY) {
      candidates.push({ bin: 'wl-copy', args: [] });
    }
    candidates.push(
      { bin: 'xclip', args: ['-selection', 'clipboard'] },
      { bin: 'xsel', args: ['--clipboard', '--input'] }
    );
  }

  for (const { bin, args } of candidates) {
    try {
      const result = spawnSync(bin, args, {
        input: text,
        stdio: ['pipe', 'ignore', 'ignore'],
        timeout: 5000,
      });
      if (result.status === 0) return true;
    } catch {
      // Tool not found or failed — try next candidate
    }
  }

  // Last resort: OSC 52 escape sequence — works over SSH/multiplexers
  // Most terminals cap OSC 52 at ~1MB; use conservative limit.
  if (
    process.platform !== 'win32' &&
    Buffer.byteLength(text, 'utf-8') <= 100_000
  ) {
    try {
      const b64 = Buffer.from(text, 'utf-8').toString('base64');
      writeFileSync('/dev/tty', `\x1b]52;c;${b64}\x07`);
      return true;
    } catch {
      // /dev/tty not available or write failed
    }
  }

  return false;
}
/**
 * Run effect for a command.
 * Returns true if the effect handled its own messaging (suppresses dispatcher step 4).
 */
export function runEffect(
  cmd: AvailableCommand,
  result: CommandResult | null,
  ctx: CommandContext,
  args: string
): boolean {
  const cmdName = cmd.name.replace(/^\//, '');
  let effectName = commandEffects[cmdName as CommandName];
  // Route /chat new [prompt] to the dedicated newSession handler
  if (cmdName === 'chat' && (args === 'new' || args.startsWith('new '))) {
    effectName = 'newSession';
  }
  if (effectName) {
    return effectHandlers[effectName]?.(result, ctx, cmd, args) === true;
  }
  return false;
}
