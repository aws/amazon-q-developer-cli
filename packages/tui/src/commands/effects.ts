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
import { ModeChangeSource } from '../types/generated/chat-cli.js';
import { KAS_DEFAULT_AGENT_NAME } from '../constants/agents.js';
import {
  enrichTurnsWithPreview,
  type TurnMessage,
} from '../utils/rewind-preview.js';
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
import { runSessionLoad } from './session-load.js';
import { Kiro } from '../kiro.js';
import { Settings } from '../constants/settings.js';
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

import { resolve } from 'path';
import { homedir } from 'os';
import { openTranscriptInPager } from '../utils/open-transcript.js';
import {
  serializeConversation,
  type TranscriptFormat,
} from '../utils/serialize-conversation.js';
import { findSettingsSubcommand } from './settings-subcommands.js';
import {
  getVerboseConfig,
  getVerboseDisplay,
  setVerboseConfig,
  validateTokens,
  VERBOSE_CATEGORIES,
  applyDensityPreset,
  DENSITY_PRESETS,
  DENSITY_DISPLAY,
  DENSITY_FILTERS,
  DEFAULT_DISPLAY,
  type ToolArgsMode,
  type DensityPreset,
} from '../lite/verbose.js';
import {
  getCurrentTitle,
  setUserTitle,
  clearUserTitle,
  isTerminalTitleEnabled,
} from '../utils/terminal-title.js';

/** Effect handler function. Returns true if it handled its own messaging. */
export type EffectHandler = (
  result: CommandResult | null,
  ctx: CommandContext,
  cmd: AvailableCommand,
  args: string
) => boolean | void | Promise<boolean | void>;

/** One of the four /verbosity truncation knobs. Char caps apply per-value
 *  (chip line, individual string values inside block args, single output
 *  rows); line caps apply to the number of visual rows below the tool name. */
type TruncationField =
  | 'argsLines'
  | 'argsChars'
  | 'outputLines'
  | 'outputChars';

/** Extract command name from TuiCommand union type */
type CommandName = TuiCommand['command'] | 'spawn' | 'switch' | 'spec';

/** Effect names - semantic actions the TUI can perform */
type EffectName =
  | 'updateModel'
  | 'updateEffort'
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
  | 'showGoalPanel'
  | 'showSettingsMenu'
  | 'switchToTui'
  | 'showChangelogPanel'
  | 'showSessionId'
  | 'showStatsPanel'
  | 'switchToGuideAgent'
  | 'switchToLite'
  | 'switchToPlanMode'
  | 'verbosityConfig'
  | 'rewindAction'
  | 'updateTitle';

/**
 * Command → Effect mapping.
 */
const commandEffects: Partial<Record<string, EffectName>> = {
  feedback: 'showFeedbackUrl',
  help: 'showHelpPanel',
  model: 'updateModel',
  effort: 'updateEffort',
  agent: 'updateAgent',
  plan: 'switchToPlanMode',
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
  reply: 'replyEditor',
  code: 'showCodePanel',
  spawn: 'spawnSession',
  switch: 'switchSession',
  spec: 'runSpec',
  copy: 'copyToClipboard',
  transcript: 'openRawView',
  theme: 'showThemeMenu',
  settings: 'showSettingsMenu',
  tui: 'switchToTui',
  lite: 'switchToLite',
  verbosity: 'verbosityConfig',
  changelog: 'showChangelogPanel',
  'session-id': 'showSessionId',
  guide: 'switchToGuideAgent',
  goal: 'showGoalPanel',
  rewind: 'rewindAction',
  title: 'updateTitle',
};

/**
 * Module-level once-per-session flag for the `/theme has moved to /settings
 * theme` deprecation nudge. The nudge fires via `showAlert(..., 'warning')`
 * — and lite routes warning-status alerts to scrollback as System rows
 * ('success' is dropped, only 'error'/'warning' land there). Without this
 * gate, every `/theme` invocation in lite would stack a fresh deprecation
 * row in the chat log. Reset only on process exit; survives /theme menu
 * re-opens, /lite ↔ /tui swaps, and /settings → Theme drilldowns.
 */
let themeDeprecationAnnounced = false;

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
      // Lite has no transient toast; emit a System row so the model swap is
      // visible in scrollback. TUI keeps origin/main's silent behavior
      // (byte-equivalent to main) — its status footer shows the current
      // model continuously, so a confirmation row would be redundant.
      if (ctx.getUiMode?.() === 'lite') {
        ctx.announceSystem(`Switched to model: ${data.model.name}`);
      }
    }
  },

  updateEffort: (result, ctx) => {
    // KAS /effort: the executeCommand result carries the validated level
    // under data.effort. Mirror the updateModel pattern and push it to the
    // store's currentEffort slot (drives the prompt-bar chip).
    const data = result?.data as { effort?: string } | undefined;
    if (data?.effort) {
      ctx.setCurrentEffort(data.effort);
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
      const fromName = ctx.currentAgent?.name;
      const toName = data.agent.name;
      if (fromName && toName && toName !== fromName) {
        ctx.kiro.sendModeChanged({
          fromMode: fromName,
          toMode: toName,
          source: ModeChangeSource.SlashCommand,
          sessionId: ctx.kiro.sessionId,
        });
      }
      ctx.setCurrentAgent(data.agent);
      if (ctx.getUiMode?.() === 'lite') {
        ctx.announceSystem(`Switched to agent: ${data.agent.name}`);
      }
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
    } else if (result?.message) {
      ctx.showAlert(result.message, 'warning', 3000);
      return true;
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
      // Merge backend commands with TUI-local commands for complete help listing.
      // Lite-only commands (meta.liteOnly) only appear in lite mode — same gating
      // CommandMenu uses for the autocomplete dropdown.
      //
      // `SlashCommand` is the only `AvailableCommand` subtype that adds
      // `source`, so checking for the field is enough to narrow.
      const isLocalHostCommand = (
        c: AvailableCommand
      ): c is SlashCommand & { source: 'local' } =>
        'source' in c && c.source === 'local';

      const inLite = ctx.getUiMode?.() === 'lite';
      const localHelpEntries = ctx.slashCommands
        .filter(isLocalHostCommand)
        .filter((c) => inLite || c.meta?.liteOnly !== true)
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

  showGoalPanel: (result, ctx) => {
    const data = result?.data as
      | {
          goal_action?: string;
          label?: string;
          definition?: { max_iterations?: number };
        }
      | undefined;
    if (data?.goal_action === 'set') {
      // Inline slash path: server holds pending_prompt_response and injects
      // the prompt server-side (same as /skills). TUI is already in streaming
      // mode from its session/prompt call. Just update goal status for the panel.
      ctx.setShowGoalPanel(false);
      const maxIterations = data.definition?.max_iterations ?? 5;
      ctx.setGoalStatus({
        state: 'active',
        iteration: 0,
        maxIterations,
        message: data.label,
        startedAt: Date.now(),
      });
      return;
    }
    if (data?.goal_action === 'clear') {
      ctx.setGoalStatus(null);
      return;
    }
    ctx.setShowGoalPanel(true);
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

  showKnowledgePanel: (result, ctx, _cmd, args) => {
    const data = result?.data as
      | { entries?: KnowledgeEntry[]; status?: string }
      | undefined;
    if (data?.entries) {
      ctx.setShowKnowledgePanel(true, data.entries, data.status);
      return;
    }
    ctx.setShowKnowledgePanel(false);
    // Subcommand failures (e.g. `/knowledge update <path>` with no contexts)
    // go to the dispatcher's tail alert — emitting here would duplicate the
    // line in lite scrollback. Bare `/knowledge` with a backend message has
    // no dispatcher alert (panel + no args is suppressed there) so we still
    // surface the message ourselves.
    if (!args && result?.message) {
      const firstLine = result.message.split('\n')[0] ?? result.message;
      ctx.showAlert(firstLine, result.success ? 'success' : 'error');
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
    // Do NOT physically wipe the terminal in lite mode. CSI 2J/3J destroys
    // the terminal scrollback buffer (pre-kiro shell history and prior
    // sessions), which is exactly the regression the user hit: /clear wiped
    // their terminal instead of clearing the conversation. The backend
    // already cleared conversation context (clear_conversation) and the
    // dispatcher surfaces a "Conversation cleared" alert — that's the
    // feedback. Lite deliberately never emits 2J/3J (see LiteLayout.tsx).
    // The KAS path below resets via resetMessages(), which bumps
    // liteScrollbackClearToken for a clean, scrollback-preserving reset.
    if (data?.sessionId) {
      // Preserve the current agent across /clear — the user expects to stay
      // on the same agent, just with a fresh conversation.
      const previousAgent = ctx.currentAgent;
      ctx.clearUIState();
      ctx.resetMessages();
      ctx.setSessionId(data.sessionId);
      if (data.currentModel) ctx.setCurrentModel(data.currentModel);
      if (previousAgent) {
        // Re-apply the previous agent to the new session
        ctx.kiro.setMode(previousAgent.name).catch(() => {
          if (data.currentAgent) ctx.setCurrentAgent(data.currentAgent);
          ctx.showAlert(
            `Failed to restore agent "${previousAgent.name}", reverted to ${KAS_DEFAULT_AGENT_NAME}`,
            'error',
            5000
          );
        });
        ctx.setCurrentAgent(previousAgent);
      } else if (data.currentAgent) {
        ctx.setCurrentAgent(data.currentAgent);
      }
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

  loadSession: (_result, ctx, _cmd, _args) => {
    // Only invoked by `rewindAction` after the backend clones the
    // session: it calls `effectHandlers.loadSession` directly with a
    // synthetic `CommandResult` carrying `{switchSession, sessionId}`.
    // /chat is owned by the v2-handlers / kas-handlers chat handlers
    // and never reaches this effect.
    const resultData = _result?.data as
      | {
          sessionId?: string;
          switchSession?: boolean;
          resetMessagesBeforeReplay?: boolean;
          suppressAgentWelcome?: boolean;
        }
      | undefined;
    if (!resultData?.switchSession || !resultData.sessionId) {
      return true;
    }
    if (!_result?.success) {
      if (_result?.message) ctx.showAlert(_result.message, 'error', 5000);
      return true;
    }
    runSessionLoad(resultData.sessionId, ctx, {
      resetMessagesBeforeReplay: resultData.resetMessagesBeforeReplay === true,
      suppressAgentWelcome: resultData.suppressAgentWelcome === true,
    });
    return true;
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
        // Lite drops 'success' alerts (app-store.ts ~3479), so showAlert
        // alone disappears in lite. The main-chat branch has no other
        // visible cue, which made the swap look like nothing happened.
        // TUI keeps the toast; lite gets a System row in scrollback.
        if (ctx.getUiMode?.() === 'lite') {
          ctx.announceSystem('Switched to main chat');
        } else {
          ctx.showAlert('Switched to main chat', 'success', 2000);
        }
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

      // Lite drops 'success' alerts, so the spawn confirmation would
      // silently disappear there. Route to scrollback in lite, keep
      // TUI's transient toast.
      if (ctx.getUiMode?.() === 'lite') {
        ctx.announceSystem(
          `Spawned ${displayName}: ${task.slice(0, 40)}${task.length > 40 ? '…' : ''}`
        );
      } else {
        ctx.showAlert(
          `Spawned ${displayName}: ${task.slice(0, 40)}${task.length > 40 ? '…' : ''}`,
          'success',
          3000
        );
      }
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

    // /spec analyze_requirements [name] — switch to spec mode and ask the
    // agent to analyze the requirements document. The agent invokes the
    // analyze_requirements tool which streams clarifying questions via
    // permission requests and updates the requirements with answers.
    if (/^analyze_requirements(\s|$)/.test(trimmed)) {
      const name = trimmed.slice(21).trim();
      const features = listSpecFeatures(workspaceRoot).filter((f) =>
        f.specDocumentPaths.some((p) => p.endsWith('requirements.md'))
      );
      if (features.length === 0) {
        ctx.showAlert(
          'No specs with requirements.md found. Use "/spec new <name>" to create one.',
          'warning',
          6000
        );
        return true;
      }
      // Exact match — execute immediately
      const exactMatch = name
        ? features.find((f) => f.featureName === name)
        : undefined;
      if (exactMatch) {
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
        const reqPath = exactMatch.specDocumentPaths.find((p) =>
          p.endsWith('requirements.md')
        )!;
        await ctx.sendMessage(
          `Analyze the requirements in ${reqPath} for ambiguities, inconsistencies, and missing acceptance criteria. Use the analyze_requirements tool.`
        );
        return true;
      }
      // No name or no exact match — show searchable picker
      ctx.setActiveCommand({
        command: {
          ...cmd,
          meta: { ...cmd.meta, inputType: 'selection' as const },
        },
        options: features.map((f) => ({
          value: `analyze_requirements ${f.featureName}`,
          label: f.featureName,
          description: describeSpecDocuments(f),
        })),
      });
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

    // Clipboard contents are invisible — without a confirmation the user
    // can't tell whether /copy worked. Lite drops 'success' alerts, so
    // the toast disappears in lite. Route to scrollback in lite, toast
    // in TUI.
    if (ctx.getUiMode?.() === 'lite') {
      ctx.announceSystem('Copied to clipboard');
    } else {
      ctx.showAlert('Copied to clipboard', 'success', 3000);
    }
    return true;
  },

  /** Open full conversation in $PAGER or save to file */
  openRawView: (_result, ctx, _cmd, args) => {
    const messages = ctx.getMessages();
    if (!messages.length) {
      ctx.showAlert('No conversation to display', 'error', 3000);
      return true;
    }

    const tokens = args.trim().split(/\s+/).filter(Boolean);
    const saving = tokens[0] === 'save';
    if (saving) tokens.shift();

    // Parse flags — everything that's not a flag is the path
    let format: TranscriptFormat = 'markdown';
    const pathParts: string[] = [];

    for (const token of tokens) {
      if (token === '--plain') {
        format = 'plaintext';
      } else if (token === '--json') {
        format = 'json';
      } else if (token.startsWith('--')) {
        ctx.showAlert(
          `Unknown flag: ${token}\nUsage: /transcript [save <path>] [--plain|--json]`,
          'error',
          5000
        );
        return true;
      } else if (saving) {
        pathParts.push(token);
      } else {
        ctx.showAlert(
          `Unknown argument: ${token}\nUsage: /transcript [save <path>] [--plain|--json]`,
          'error',
          5000
        );
        return true;
      }
    }
    const filePath = pathParts.join(' ');

    // Serialize
    const content = serializeConversation(messages, format);

    if (saving) {
      const ext = { markdown: '.md', plaintext: '.txt', json: '.json' };
      const expanded = filePath.startsWith('~/')
        ? homedir() + filePath.slice(1)
        : filePath;
      const outputPath = resolve(expanded || `transcript${ext[format]}`);
      try {
        writeFileSync(outputPath, content);
        ctx.showAlert(`Transcript saved to ${outputPath}`, 'success', 3000);
      } catch (err: any) {
        ctx.showAlert(`Failed to save: ${err.message}`, 'error', 5000);
      }
    } else {
      const ext = { markdown: 'md', plaintext: 'txt', json: 'json' } as const;
      openTranscriptInPager(messages, content, ext[format]);
    }
    return true;
  },

  showChangelogPanel: (_result, ctx) => {
    ctx.setShowChangelogPanel(true);
  },

  showSessionId: (_result, ctx) => {
    const sessionId = ctx.kiro.sessionId ?? 'none';
    // /session-id's whole point is to print the ID for the user to copy.
    // Lite drops 'success' alerts, so the toast disappears in lite —
    // putting the row in scrollback also lets the user scroll back to it
    // later. TUI keeps the long toast.
    if (ctx.getUiMode?.() === 'lite') {
      ctx.announceSystem(`Session ID: ${sessionId}`);
    } else {
      ctx.showAlert(
        sessionId !== 'none'
          ? `Session ID: ${sessionId}\nResume with: kiro-cli --resume-id ${sessionId}`
          : 'Session ID: none',
        'success',
        10000
      );
    }
    return true;
  },

  updateTitle: (_result, ctx, _cmd, args) => {
    const trimmed = (args ?? '').trim();

    // `/title` — show the current terminal title
    if (!trimmed) {
      const title = getCurrentTitle();
      if (isTerminalTitleEnabled()) {
        ctx.showAlert(`Current title: ${title}`, 'success', 4000);
      } else {
        ctx.showAlert(
          `Current title (not active): ${title} — enable via /settings display`,
          'warning',
          4000
        );
      }
      return true;
    }

    // `/title --clear` — remove the manual override and revert to auto-derived title
    if (trimmed === '--clear') {
      const result = clearUserTitle();
      if (result.ok) {
        ctx.showAlert(
          `Title cleared — showing: ${getCurrentTitle()}`,
          'success',
          4000
        );
      } else {
        ctx.showAlert(
          'Terminal title is disabled — enable via /settings display',
          'error',
          4000
        );
      }
      return true;
    }

    // `/title <text>` — set a sticky manual title override
    const result = setUserTitle(trimmed);
    if (result.ok) {
      ctx.showAlert(`Title set: ${result.title}`, 'success', 4000);
    } else if (result.reason === 'empty') {
      ctx.showAlert(
        'Title is empty after removing special characters',
        'error',
        4000
      );
    } else {
      ctx.showAlert(
        'Terminal title is disabled — enable via /settings display',
        'error',
        4000
      );
    }
    return true;
  },

  showThemeMenu: (_result, ctx, cmd, args) => {
    // Modern TUI: /theme is a legacy alias that opens main's ThemePanel
    // (the canonical entry is /settings → theme). Lite keeps its own rich
    // /theme command-menu with live preview, handled below.
    if (ctx.getUiMode?.() !== 'lite') {
      if (cmd.name === '/theme') {
        ctx.showAlert('/theme has moved to /settings theme', 'warning', 4000);
      }
      ctx.setShowThemePanel(true);
      return true;
    }
    // Deprecation notice shown ONCE per session (not per /theme invocation).
    // Skipped on in-menu selections (args !== '') and on calls chained from
    // /settings theme (cmd.name !== '/theme'). Lite routes warning-status
    // alerts to scrollback, so without the gate every /theme invocation
    // would stack a fresh row in the chat log.
    if (cmd.name === '/theme' && args === '' && !themeDeprecationAnnounced) {
      themeDeprecationAnnounced = true;
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
      ctx.setThemePreview(
        buildCurrentPreview(prefs, fallbackDiff, kiroSafe.colors.brand)
      );
      // ESC from the custom menu returns to the bare /theme top-level menu.
      // Empty string is the route — CommandMenu re-dispatches `/theme` with
      // no args, which falls through to the top-level menu builder below.
      ctx.setThemeReturnOnEscape('');
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
      ctx.setThemePreview(
        buildCurrentPreview(prefs, fallbackDiff, kiroSafe.colors.brand)
      );
      // ESC returns to the /theme custom menu (one level up). The 'custom'
      // route is re-dispatched verbatim by CommandMenu so the user lands
      // exactly where they descended from.
      ctx.setThemeReturnOnEscape('custom');
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
      ctx.setThemePreview(
        buildCurrentPreview(prefs, fallbackDiff, kiroSafe.colors.brand)
      );
      ctx.setThemeReturnOnEscape('custom');
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
      ctx.setThemePreview(
        buildCurrentPreview(prefs, fallbackDiff, kiroSafe.colors.brand)
      );
      ctx.setThemeReturnOnEscape('custom');
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
      ctx.setThemePreview(
        buildCurrentPreview(updatedPrefs, fallbackDiff, kiroSafe.colors.brand)
      );
      // The menu we're re-opening IS the /theme custom menu, so ESC should
      // return to bare /theme (top-level) — same route as the direct
      // `args === 'custom'` path above.
      ctx.setThemeReturnOnEscape('');
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
    // Top-level menu — ESC fully closes the overlay (no parent above).
    ctx.setThemeReturnOnEscape(null);

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
      const resolveEffect = (name: string) => {
        const handler = effectHandlers[name as EffectName];
        if (!handler) {
          throw new Error(`Unknown effect handler: ${name}`);
        }
        return handler;
      };
      // Exact match first — preserves the colon-form values
      // (e.g. `terminal:interrupt:steer`) the menu rows dispatch directly.
      let sub = findSettingsSubcommand(args);
      let arg = '';
      // Fall back to "subcommand + trailing section", e.g.
      // `/settings verbosity truncation`. The first space-delimited word is the
      // subcommand; the rest is forwarded so the handler can drill straight
      // into a nested menu (mirrors the breadcrumb so nested menus are
      // reachable as typed subcommands).
      if (!sub) {
        const space = args.indexOf(' ');
        if (space !== -1) {
          const head = args.slice(0, space);
          const candidate = findSettingsSubcommand(head);
          if (candidate) {
            sub = candidate;
            arg = args.slice(space + 1).trim();
          }
        }
      }
      if (!sub) {
        ctx.showAlert(`Unknown settings subcommand: ${args}`, 'error', 3000);
        return true;
      }
      void Promise.resolve(
        sub.handle({
          ctx,
          settingsCommand: cmd,
          resolveEffect,
          arg,
        })
      );
      return true;
    }

    // Bare /settings opens the shared SettingsPanel overlay in BOTH modes;
    // the panel handles its own item rendering and routing to sub-panels.
    // Lite renders the same panel via <BackendPanels> so the two modes stay
    // 1:1 (breadcrumb titles, panel heights, ESC-back). The lite-only
    // `verbosity` row is added inside the panel's model, gated on uiMode.
    ctx.setShowSettingsPanel(true);
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

  switchToLite: (_result, ctx) => {
    // Gated on Feature::Lite rollout (internal + nightly). Outside the
    // cohort, /lite is a no-op so stable users keep the modern TUI
    // behavior they had before this branch existed.
    if (process.env.KIRO_LITE_ROLLOUT_ENABLED !== '1') {
      ctx.announceSystem('Lite mode is not available in this build');
      return;
    }
    const fromMode = ctx.getUiMode?.() ?? 'tui';
    // tui→lite clears scrollback and re-renders the full conversation in
    // lite form (symmetric with lite→tui). setUiMode bumps the clear
    // token; LiteLayout + ConversationView both observe it, wipe their
    // module-level singletons, and reset twinki's cursor. The user gets
    // consistent lite styling (You:/<agent>: headers, current verbosity,
    // current theme) across every message rather than a half-and-half
    // mix of TUI-styled history + lite-styled new rows.
    ctx.setUiMode?.('lite');
    if (fromMode !== 'lite') {
      ctx.kiro.sendUiModeChanged({
        from: fromMode,
        to: 'lite',
        source: ModeChangeSource.SlashCommand,
        sessionId: ctx.kiro.sessionId,
      });
    }
    ctx.announceSystem('Switched to lite mode');
  },

  switchToTui: (_result, ctx) => {
    // /tui from lite swaps to TUI; from TUI it falls through to the
    // info panel (origin/main behavior). Symmetric with /lite.
    if (ctx.getUiMode?.() === 'lite') {
      ctx.setUiMode?.('tui');
      ctx.kiro.sendUiModeChanged({
        from: 'lite',
        to: 'tui',
        source: ModeChangeSource.SlashCommand,
        sessionId: ctx.kiro.sessionId,
      });
      ctx.announceSystem('Switched to TUI mode');
      return;
    }
    ctx.setShowTuiPanel(true);
  },

  /**
   * /verbosity: configure lite-mode rendering.
   *
   * The interactive menu is a sectioned drilldown:
   *   top → density preset, sub-menus (Tool calls, Subagent,
   *         Output bar), Reset
   *   tool drilldown → Reasoning, Args mode, Elapsed
   *   subagent drilldown → Pipeline, Prompts, Roles, Deps, Responses
   *   output drilldown → per-category filter toggles
   *
   * Power-user CLI form (typed as args after the command) is preserved:
   *   /verbosity on|off (aliases for filters: ['all'] / [])
   *   /verbosity status / /verbosity all
   *   /verbosity only|add|remove <list>
   *   /verbosity density <minimal|lean|normal>
   *
   * Rejected outside lite mode — the renderer hooks only run in <LiteLayout>.
   */
  verbosityConfig: (_result, ctx, cmd, args) => {
    if (ctx.getUiMode?.() !== 'lite') {
      ctx.showAlert('/verbosity is only available in lite mode', 'error', 3000);
      return true;
    }

    // Resolve the canonical /verbosity SlashCommand from the registry so the
    // menu's command chip and CommandMenu's `command.name === '/verbosity'`
    // detection (Ctrl+P preview toggle, density-row draft highlight, stale
    // state reset) work identically regardless of how the handler was
    // reached — direct typing or `/settings verbosity`. Without this, entering
    // via /settings produces an activeCommand whose name is `/settings`, and
    // CommandMenu's checks silently fail.
    //
    // Mirrors the showThemeMenu pattern at the top of that handler:
    // resolve the canonical cmd, bail silently if not registered (a
    // production-impossible case kept defensive for tests). Suppressed in
    // tests that only register the settings cmd — those tests fall back to
    // `cmd`, the legacy behavior.
    const verbosityCmd =
      ctx.slashCommands.find((c) => c.name === '/verbosity') ?? cmd;

    // Case folding for command matching. Internal-dispatch forms (`menu:*`,
    // `set:*`, `category:*`, `filter:*`, `reset:*`) never come from user
    // typing — the menu writes them with exact casing — so we preserve them
    // unchanged. Everything else lowercases its first whitespace-bounded
    // word so verbs like `ON`, `ALL`, `Density` match the routing branches,
    // while filter tokens after the first word keep their original case
    // (MCP tool names like `mcp__nova-memory-mcp__remember` are
    // case-sensitive).
    const rawTrimmed = args.trim();
    const isInternalDispatch =
      /^(menu|set|category|filter|reset):/.test(rawTrimmed) &&
      !/\s/.test(rawTrimmed);
    let trimmed: string;
    if (isInternalDispatch) {
      trimmed = rawTrimmed;
    } else {
      const firstWs = rawTrimmed.search(/\s/);
      trimmed =
        firstWs === -1
          ? rawTrimmed.toLowerCase()
          : rawTrimmed.slice(0, firstWs).toLowerCase() +
            rawTrimmed.slice(firstWs);
    }
    const cfg = getVerboseConfig();

    // Active density preset = whichever preset's display config AND filter
    // list exactly match the saved one. `null` when the user has hand-toggled
    // into a shape that doesn't correspond to any preset — the menu shows
    // "Density: custom" in that case.
    //
    // `default` and `full` share an identical display config; the
    // differentiator is the filter list (DENSITY_FILTERS — `['all']` for
    // full, `[]` for the rest). When the display matches multiple presets,
    // the filter list disambiguates.
    const detectActivePreset = (): DensityPreset | null => {
      const curCfg = getVerboseConfig();
      const cur = curCfg.display ?? DEFAULT_DISPLAY;
      for (const preset of DENSITY_PRESETS) {
        if (!sameDisplay(cur, DENSITY_DISPLAY[preset])) continue;
        if (!sameFilters(curCfg.filters, DENSITY_FILTERS[preset])) continue;
        return preset;
      }
      return null;
    };

    // Short-form summary used in menu rows (no truncation — the row width
    // handles overflow). `none` for empty, `all` for the full-list shortcut,
    // otherwise comma-joined.
    const fmtFilters = (f: string[]) => {
      if (f.length === 0) return 'none';
      if (f.length === 1 && f[0] === 'all') return 'all';
      return f.join(', ');
    };
    // Long-form for system announcements: collapses with a count when the
    // joined form would wrap, since the lite renderer's word-break splits on
    // character count rather than commas. Threshold derives from the terminal
    // width (with a 120-column fallback) minus a generous prefix budget.
    const fmtFiltersForAnnounce = (f: string[]) => {
      if (f.length === 0) return 'none';
      if (f.length === 1 && f[0] === 'all') return 'all';
      const joined = f.join(', ');
      const cols = process.stdout.columns ?? 120;
      const budget = Math.max(40, cols - 30);
      if (joined.length <= budget) return joined;
      const head: string[] = [];
      let used = 0;
      for (const t of f) {
        const next = used === 0 ? t.length : used + 2 + t.length;
        if (next > budget - 12) break;
        head.push(t);
        used = next;
      }
      const remaining = f.length - head.length;
      const headStr = head.length > 0 ? head.join(', ') + ', ' : '';
      return `${f.length} (${headStr}... +${remaining} more)`;
    };
    // Status announcements no longer carry an ON/OFF prefix — the filter list
    // is the source of truth, and an empty list communicates "off" on its own.
    const showStatus = (msg?: string) => {
      const cur = getVerboseConfig();
      if (msg) {
        ctx.announceSystem(
          `${msg} · filters: ${fmtFiltersForAnnounce(cur.filters)}`
        );
        return;
      }
      const density = detectActivePreset() ?? 'custom';
      ctx.announceSystem(
        `verbosity · filters: ${fmtFiltersForAnnounce(cur.filters)} · density: ${density}`
      );
    };

    // Pretty-print a numeric cap for menu summaries and value-row descriptions.
    // Both `null` and any non-positive value mean "unbounded" — we never want
    // a 0 or negative value to render as a plausible cap. `unit` defaults to
    // `lines` for backward-compatible use; chars caps pass `chars`.
    const fmtCap = (
      cap: number | null,
      unit: 'lines' | 'chars' = 'lines'
    ): string => (cap == null || cap <= 0 ? 'unlimited' : `${cap} ${unit}`);

    // Stash the parent route consumed by CommandMenu's ESC handler. Pass
    // `null` for the top menu so the next ESC fully exits, otherwise set
    // `'menu:top'` so ESC navigates one level up. The store flag is one-shot:
    // CommandMenu clears it on consume, so we must rewrite it every time we
    // re-open a sub-menu (e.g. after a toggle re-opens the same menu).
    const setReturn = (route: string | null) => {
      ctx.setVerboseReturnOnEscape?.(route);
    };

    // Open a menu with the given options, reusing the same SlashCommand
    // shape so the menu component renders consistently. The header chip the
    // user sees in the prompt area is the command name (`/verbosity`); the
    // submenu state lives entirely in the option set.
    //
    // `initialIndex` chooses which row the cursor lands on. Defaults to 0;
    // submenus always pass 0, the top menu passes the row of the submenu the
    // user just exited (so ESC + Enter is idempotent). Clamped by the menu
    // component to stay within the option list.
    const openMenuWith = (
      options: Array<{
        value: string;
        label: string;
        description?: string;
        group?: string;
      }>,
      initialIndex = 0,
      previewKey?: string
    ) => {
      ctx.setActiveCommand({
        command: {
          ...verbosityCmd,
          meta: {
            ...verbosityCmd.meta,
            inputType: 'selection' as const,
            searchable: false,
          },
        },
        options,
        initialIndex,
        previewKey,
      });
    };

    const onOff = (b: boolean) => (b ? '[on]' : '[off]');

    // Top-menu row index for each submenu key — used by ESC-back to land the
    // cursor on the row the user descended from. Must stay in sync with the
    // option order in `openTopMenu`. `thinking` is the inline toggle row,
    // not a submenu — included so re-opens after toggling land on the same
    // row (otherwise the cursor jumps to row 0). The standalone "Reset to
    // defaults" row is gone — picking the `default` preset from the density
    // menu now serves that purpose, and the menu has a top-level entry
    // confirmation when changing presets.
    const TOP_ROW_BY_KEY: Record<string, number> = {
      density: 0,
      tool: 1,
      subagent: 2,
      thinking: 3,
      tasks: 4,
      output: 5,
      truncation: 6,
    };

    const openTopMenu = (fromKey?: string) => {
      setReturn(null);
      const cur = getVerboseConfig();
      // Use getVerboseDisplay so the rendered menu reflects the unified
      // chat.showThinking value from cli.json — without this, the row
      // would show whatever lite_verbose.json was last written with even
      // if /settings → Display had since toggled the modern-TUI side.
      // Filter rows below still read from `cur.filters`.
      const display = getVerboseDisplay();
      const preset = detectActivePreset();
      const presetLabel = preset ?? 'custom';
      // Sub-menu summaries — short one-liners so the user sees current state
      // without having to drill in. Compact wording (`args: inline`) wins
      // over verbose key:value because the menu rows have limited width.
      const toolSummary = `args: ${display.toolArgsMode} · reasoning: ${display.showToolReasoning ? 'on' : 'off'} · elapsed: ${display.showElapsed ? 'on' : 'off'}`;
      // Summary surfaces only the user-meaningful knobs (steps + responses
      // + full output). `roles` / `prompts` / `deps` are nested under the
      // step list and adjust in lockstep with it — calling them out
      // separately bloats the summary.
      const subSummaryParts: string[] = [];
      if (display.subagent.pipeline) {
        const sublist: string[] = [];
        if (display.subagent.prompts) sublist.push('instructions');
        if (display.subagent.roles) sublist.push('roles');
        const stepLabel =
          sublist.length > 0 ? `steps + ${sublist.join(' + ')}` : 'steps';
        subSummaryParts.push(stepLabel);
      }
      if (display.subagent.responses) subSummaryParts.push('summary');
      const fullOutputOn =
        cur.filters.includes('all') || cur.filters.includes('subagent');
      if (fullOutputOn) subSummaryParts.push('full output');
      const subSummary =
        subSummaryParts.length === 0
          ? '(all hidden)'
          : subSummaryParts.join(' · ');
      const outSummary = fmtFilters(cur.filters);
      // Compact two-axis summary: lines/chars per cap, since both can be
      // independently active. Longest form fits within typical menu width.
      const truncSummary = `args ${fmtCap(display.argsMaxLines)}/${fmtCap(display.argsMaxChars, 'chars')} · output ${fmtCap(display.outputMaxLines)}/${fmtCap(display.outputMaxChars, 'chars')}`;

      openMenuWith(
        [
          {
            value: 'menu:density',
            label: 'Density preset',
            description: presetLabel,
            group: 'Density',
          },
          {
            value: 'menu:tool',
            label: 'Tool calls',
            description: toolSummary,
            group: 'Sections',
          },
          {
            value: 'menu:subagent',
            label: 'Subagent',
            description: subSummary,
            group: 'Sections',
          },
          {
            value: 'set:showThinkingContent',
            label: 'Thinking content',
            description: onOff(display.showThinkingContent),
            group: 'Sections',
          },
          {
            value: 'set:showTasks',
            label: 'Task list',
            description: onOff(display.showTasks),
            group: 'Sections',
          },
          {
            value: 'menu:output',
            label: 'Show output',
            description: outSummary,
            group: 'Sections',
          },
          {
            value: 'menu:truncation',
            label: 'Truncation',
            description: truncSummary,
            group: 'Sections',
          },
        ],
        fromKey ? (TOP_ROW_BY_KEY[fromKey] ?? 0) : 0,
        'top'
      );
    };

    // Per-preset description shared between the density menu rows and the
    // confirmation submenu. Centralized so the confirm row title can match
    // the row the user just selected without drift.
    const PRESET_DESC: Record<DensityPreset, string> = {
      minimal: 'name only · no args, no reasoning',
      lean: 'inline arg chip, no reasoning, full elapsed',
      default: 'reasoning + block args + full subagent (out-of-the-box)',
      full: '1:1 of what the parent agent sees · all filters on · no truncation',
    };

    /**
     * Density menu — the primary entry point when a preset is active.
     *
     * Lists all four presets plus a `Custom` row that opens the config menu.
     * The `← back` row is also Custom-equivalent so users have an obvious
     * way to drill into the per-knob menu without learning the explicit
     * `Custom` label.
     *
     * Highlighting a preset row should draft-preview it inline — wired in
     * CommandMenu via the highlight handler, not here. Selecting a preset
     * routes to the `density:confirm:<preset>` confirmation submenu rather
     * than committing immediately, mirroring the old reset-confirm gate.
     */
    const openDensityMenu = (initialIndex = 0) => {
      // ESC from the density menu fully exits — there's no parent above it
      // when it's the entry point. (Reached via the Custom flow → config
      // menu → density submenu, ESC behavior is governed by setReturn
      // earlier.) Default-clear so accidental cursor states from prior
      // submenus don't persist.
      setReturn(null);
      const active = detectActivePreset();
      const options: Array<{
        value: string;
        label: string;
        description: string;
      }> = DENSITY_PRESETS.map((p) => ({
        // `menu:`-prefixed because this is internal menu navigation (opens
        // the per-preset confirmation submenu), not a CLI form. The route
        // handler at the bottom of this effect only matches the menu form;
        // dropping the prefix here used to silently fall through to the
        // "Unknown subcommand" error for every preset.
        value: `menu:density:confirm:${p}`,
        label: p,
        description:
          active === p ? `[active] · ${PRESET_DESC[p]}` : PRESET_DESC[p],
      }));
      // Custom — explicit row so users know how to tweak individual knobs
      // without thinking "what menu am I missing". Routes to the config
      // menu (the prior top-menu drilldown).
      options.push({
        value: 'menu:config',
        label: 'custom',
        description:
          active == null
            ? '[active] · tweak individual settings'
            : 'tweak individual settings',
      });
      openMenuWith(options, initialIndex, 'density');
    };

    /**
     * Confirmation gate for picking a density preset. Mirrors the old
     * reset-to-defaults confirm flow — Cancel comes first so the default
     * cursor lands on a safe row, Yes commits the preset (display + filters)
     * and re-opens the density menu.
     *
     * `which` carries the preset the user picked so the confirmation can
     * name it specifically and the route stays self-describing for ESC
     * navigation.
     */
    const openPresetConfirmMenu = (which: DensityPreset) => {
      setReturn('menu:density');
      openMenuWith(
        [
          {
            value: 'menu:density',
            label: 'Cancel',
            description: '',
            group: `Confirm preset: ${which}`,
          },
          {
            value: `density:apply:${which}`,
            label: `Yes, switch to ${which}`,
            description: PRESET_DESC[which],
            group: `Confirm preset: ${which}`,
          },
          { value: 'menu:density', label: '← back', description: '' },
        ],
        0,
        // The preview pane reads VerbosityPreviewKey 'density' for any
        // preset-related menu — same fixture set as the density menu itself,
        // since the confirmation is essentially "look at this preview, are
        // you sure?".
        'density'
      );
    };

    const openToolMenu = () => {
      setReturn('menu:top:tool');
      const display = getVerboseDisplay();
      const argModeRow = (mode: ToolArgsMode) => ({
        value: `set:toolArgsMode:${mode}`,
        label: `Args: ${mode}`,
        description: display.toolArgsMode === mode ? '[active]' : '',
        group: 'Args display',
      });
      openMenuWith(
        [
          {
            value: 'set:showToolReasoning',
            label: 'Reasoning ("why")',
            description: onOff(display.showToolReasoning),
            group: 'Per-tool toggles',
          },
          {
            value: 'set:showElapsed',
            label: 'Elapsed time',
            description: onOff(display.showElapsed),
            group: 'Per-tool toggles',
          },
          {
            value: 'set:showWriteDiffs:tool',
            label: 'Write diffs',
            description: onOff(display.showWriteDiffs),
            group: 'Per-tool toggles',
          },
          argModeRow('off'),
          argModeRow('inline'),
          argModeRow('block'),
          { value: 'menu:top:tool', label: '← back', description: '' },
        ],
        0,
        'tool'
      );
    };

    const openSubagentMenu = () => {
      setReturn('menu:top:subagent');
      const cur = getVerboseConfig();
      const sub = (cur.display ?? DEFAULT_DISPLAY).subagent;
      const row = (key: keyof typeof sub, label: string) => ({
        value: `set:subagent:${key}`,
        label,
        description: onOff(sub[key]),
        group: 'Subagent display',
      });
      // Gating rule: prompts and roles only emit anything when the master
      // step list is on (the renderer wraps both in `if (sub.pipeline && ...)`),
      // so showing them while pipeline is off would be a dead toggle. When
      // off, drop the dependent rows so the menu reflects what each row
      // can actually accomplish.
      const stepRows = sub.pipeline
        ? [
            row('prompts', 'Show step instructions'),
            row('roles', 'Show step role labels'),
          ]
        : [];
      // Subagent's full taskResult body is gated by the `subagent` filter
      // (mirrors what `output` does for tool output bars). Surface it here
      // too so users don't have to know the filter→subagent connection.
      const filterSet = new Set(cur.filters);
      const fullOutputOn =
        cur.filters.includes('all') || filterSet.has('subagent');
      openMenuWith(
        [
          row('pipeline', 'Show subagent steps'),
          ...stepRows,
          row('responses', 'Show response summary'),
          {
            value: 'set:subagent:fullOutput',
            label: 'Show full output (verbose)',
            description: onOff(fullOutputOn),
            group: 'Subagent display',
          },
          { value: 'menu:top:subagent', label: '← back', description: '' },
        ],
        0,
        'subagent'
      );
    };

    // Truncation submenu exposes four independent caps — line and char limits
    // for both args and output — so users can pick the unit that maps best
    // to what they care about. Each row routes to the numeric editor
    // (rendered by CommandMenu when previewKey ends with `:edit`).
    //
    // Per-line vs per-char: line caps are visual-row counts (`argsMaxLines`,
    // `outputMaxLines`); char caps cut individual values at N characters
    // (`argsMaxChars`, `outputMaxChars`). The chip in inline mode and any
    // long string value inside block-mode use the char cap.
    const TRUNC_HEADINGS: Record<TruncationField, string> = {
      argsLines: 'Tool args · lines',
      argsChars: 'Tool args · chars per value',
      outputLines: 'Tool output · lines',
      outputChars: 'Tool output · chars per line',
    };
    const truncCap = (
      display: typeof DEFAULT_DISPLAY,
      which: TruncationField
    ): number | null => {
      switch (which) {
        case 'argsLines':
          return display.argsMaxLines;
        case 'argsChars':
          return display.argsMaxChars;
        case 'outputLines':
          return display.outputMaxLines;
        case 'outputChars':
          return display.outputMaxChars;
      }
    };
    const openTruncationMenu = () => {
      setReturn('menu:top:truncation');
      const display = getVerboseDisplay();
      openMenuWith(
        [
          {
            value: 'menu:truncation:argsLines:edit',
            label: 'Args · lines',
            description: fmtCap(display.argsMaxLines),
            group: 'Tool args',
          },
          {
            value: 'menu:truncation:argsChars:edit',
            label: 'Args · chars per value',
            description: fmtCap(display.argsMaxChars, 'chars'),
            group: 'Tool args',
          },
          {
            value: 'menu:truncation:outputLines:edit',
            label: 'Output · lines',
            description: fmtCap(display.outputMaxLines),
            group: 'Tool output',
          },
          {
            value: 'menu:truncation:outputChars:edit',
            label: 'Output · chars per line',
            description: fmtCap(display.outputMaxChars, 'chars'),
            group: 'Tool output',
          },
          { value: 'menu:top:truncation', label: '← back', description: '' },
        ],
        0,
        'truncation'
      );
    };

    // Open the numeric editor for one of the four caps. The "menu" itself is
    // a no-op option list — CommandMenu renders the VerbosityTruncationEditor
    // component when previewKey ends with `:edit`. The single menu row is a
    // dummy placeholder so navigation still works (Enter/Escape behave the
    // same as any other menu).
    const openTruncationEditor = (which: TruncationField) => {
      // Esc from the editor returns to the Truncation submenu, NOT the top.
      setReturn('menu:truncation');
      const display = getVerboseDisplay();
      const cur = truncCap(display, which);
      const heading = TRUNC_HEADINGS[which];
      const unit: 'lines' | 'chars' =
        which === 'argsChars' || which === 'outputChars' ? 'chars' : 'lines';
      // Single placeholder option — the editor handles all keypresses, the
      // menu's own option list is never used. We still emit one row so the
      // menu component doesn't render an empty selection.
      openMenuWith(
        [
          {
            value: `menu:truncation:${which}:edit`,
            label: heading,
            description: fmtCap(cur, unit),
            group: heading,
          },
        ],
        0,
        `truncation:${which}:edit`
      );
    };

    const openOutputMenu = () => {
      setReturn('menu:top:output');
      const cur = getVerboseConfig();
      const isAll = cur.filters.includes('all');
      const filterSet = new Set(cur.filters);
      // Master row toggles between every-on (`['all']`) and every-off (`[]`).
      // Label flips so the row reads as the action it will perform: when all
      // are currently on, the row says "none" because pressing it turns
      // everything off. Inverse for the off state.
      const masterLabel = isAll ? 'none' : 'all';
      const masterDesc = isAll
        ? '[active] · every tool · press to clear'
        : 'turn every tool on';
      openMenuWith(
        [
          {
            value: 'filter:all',
            label: masterLabel,
            description: masterDesc,
            group: 'Filter',
          },
          ...VERBOSE_CATEGORIES.map((category) => ({
            value: `category:${category}`,
            label: category,
            description: onOff(isAll || filterSet.has(category)),
            group: 'Filter',
          })),
          { value: 'menu:top:output', label: '← back', description: '' },
        ],
        0,
        'output'
      );
    };

    // ── Routing ────────────────────────────────────────────────────────────

    // Bare /verbosity: smart entry. Lands in the density menu when an active
    // preset is detected (the simple, common case), else in the config menu
    // (custom configurations want the per-knob view). The two routes
    // `menu:density` and `menu:config` let internal dispatch reach either
    // one explicitly without going through the smart router.
    if (trimmed === '' || trimmed === 'config') {
      if (trimmed === 'config') {
        openTopMenu();
      } else if (detectActivePreset() != null) {
        openDensityMenu();
      } else {
        openTopMenu();
      }
      return true;
    }
    // Explicit config-menu route — used by the Custom row in the density
    // menu and by ESC-back from a submenu. Keeps the bare /verbosity smart
    // routing separate from "I explicitly want the per-knob menu".
    if (trimmed === 'menu:config') {
      openTopMenu();
      return true;
    }
    if (trimmed === 'menu:top') {
      openTopMenu();
      return true;
    }
    // `menu:top:<key>` — used by ESC-back from a submenu (and by the `← back`
    // rows) so the cursor lands on the row representing that submenu instead
    // of resetting to row 0.
    if (trimmed.startsWith('menu:top:')) {
      const fromKey = trimmed.slice('menu:top:'.length);
      openTopMenu(fromKey);
      return true;
    }
    if (trimmed === 'menu:density') {
      openDensityMenu();
      return true;
    }
    // Confirmation gate for a density preset. `menu:density:confirm:<preset>`
    // is what the density menu rows route to on Enter — opens the per-preset
    // confirm submenu without committing.
    {
      const confirmMatch = trimmed.match(/^menu:density:confirm:([a-z]+)$/);
      if (confirmMatch) {
        const preset = confirmMatch[1] as DensityPreset;
        if (!DENSITY_PRESETS.includes(preset)) {
          ctx.showAlert(`Unknown density preset: ${preset}`, 'error', 3000);
          return true;
        }
        openPresetConfirmMenu(preset);
        return true;
      }
    }
    if (trimmed === 'menu:tool') {
      openToolMenu();
      return true;
    }
    if (trimmed === 'menu:subagent') {
      openSubagentMenu();
      return true;
    }
    if (trimmed === 'menu:output') {
      openOutputMenu();
      return true;
    }
    if (trimmed === 'menu:truncation') {
      openTruncationMenu();
      return true;
    }
    {
      const editMatch = trimmed.match(
        /^menu:truncation:(argsLines|argsChars|outputLines|outputChars):edit$/
      );
      if (editMatch) {
        openTruncationEditor(editMatch[1] as TruncationField);
        return true;
      }
    }

    // Friendly section names — let users jump straight into a sub-menu by its
    // breadcrumb name, both as `/verbosity <section>` and (forwarded by the
    // settings router) `/settings verbosity <section>`. Mirrors the nested
    // breadcrumb so the menus are discoverable as typed subcommands. The first
    // word was already lowercased by the case-folding above, so multi-word
    // forms like "tool calls" compare in lower case here.
    //
    // `density` is intentionally NOT here: bare `density` is the existing CLI
    // form (`density <preset>`) handled below, and the density menu is already
    // the smart-entry default for bare `/verbosity` on a preset.
    {
      const SECTION_OPENERS: Record<string, () => void> = {
        tool: openToolMenu,
        tools: openToolMenu,
        'tool calls': openToolMenu,
        subagent: openSubagentMenu,
        subagents: openSubagentMenu,
        output: openOutputMenu,
        truncation: openTruncationMenu,
      };
      const opener = SECTION_OPENERS[trimmed];
      if (opener) {
        opener();
        return true;
      }
    }

    // /verbosity on / off remain as CLI aliases for backward compat — they
    // map onto the filter list since the master enabled toggle is gone.
    if (trimmed === 'on') {
      setVerboseConfig({ filters: ['all'] });
      showStatus();
      return true;
    }
    if (trimmed === 'off') {
      setVerboseConfig({ filters: [] });
      showStatus();
      return true;
    }
    if (trimmed === 'status') {
      showStatus();
      return true;
    }
    if (trimmed === 'all' || trimmed === 'filter:all') {
      // CLI form (`/verbosity all`) is a one-shot reset to every-on — the user
      // typed "all" so we honor that literal intent. Menu row routes here as
      // `filter:all` and is the toggle: every-on flips to every-off, anything
      // else flips to every-on. The label on the menu row swaps too so the
      // user sees the action that will fire.
      if (trimmed === 'all') {
        setVerboseConfig({ filters: ['all'] });
        showStatus('verbosity: filters reset');
        return true;
      }
      const cur = getVerboseConfig();
      const wasAll = cur.filters.length === 1 && cur.filters[0] === 'all';
      setVerboseConfig({ filters: wasAll ? [] : ['all'] });
      openOutputMenu();
      return true;
    }
    // CLI-form `/verbosity reset` is preserved as a power-user shortcut for
    // the `default` preset's exact effect (display = DEFAULT_DISPLAY,
    // filters = []). It bypasses the menu confirmation since the user opted
    // in by typing the verb. The standalone Reset row in the menu is gone —
    // picking the `default` preset now serves that purpose.
    if (trimmed === 'reset') {
      applyDensityPreset('default');
      ctx.announceSystem('verbosity: reset to defaults');
      openTopMenu();
      return true;
    }

    // Bare preset name (e.g. `/verbosity full`) routes to the same code path
    // as `/verbosity density <preset>`. The menu surfaces preset names as
    // first-class entries, so users naturally type the bare name; rejecting
    // them with "Unknown subcommand" was a sharp edge.
    if (DENSITY_PRESETS.includes(trimmed as DensityPreset)) {
      applyDensityPreset(trimmed as DensityPreset);
      ctx.announceSystem(`verbosity: density set to ${trimmed}`);
      return true;
    }

    // Density: CLI (`density <preset>`) commits immediately; menu form is
    // `density:apply:<preset>` (post-confirmation Yes). `density:<preset>`
    // is preserved as a CLI shortcut form. Both apply the preset's display
    // AND filter list — picking a preset is a clean reset to that preset's
    // full intent.
    const densityCliMatch = trimmed.match(/^density(?:\s+(.+))?$/);
    const densityMenuMatch = trimmed.match(/^density:([a-z]+)$/);
    const densityApplyMatch = trimmed.match(/^density:apply:([a-z]+)$/);
    if (densityCliMatch || densityMenuMatch || densityApplyMatch) {
      const preset = (
        densityApplyMatch?.[1] ??
        densityMenuMatch?.[1] ??
        densityCliMatch?.[1] ??
        ''
      ).trim();
      if (!preset) {
        ctx.showAlert(
          `density needs a preset: ${DENSITY_PRESETS.join(', ')}`,
          'error',
          4000
        );
        return true;
      }
      if (!DENSITY_PRESETS.includes(preset as DensityPreset)) {
        ctx.showAlert(`Unknown density preset: ${preset}`, 'error', 3000);
        return true;
      }
      applyDensityPreset(preset as DensityPreset);
      ctx.announceSystem(`verbosity: density set to ${preset}`);
      // Picking a preset is a finish action — close the menu entirely so
      // the user lands back at the prompt instead of bouncing back into the
      // density submenu. The status announcement above is what tells them
      // the change took effect; an open menu after commit had been read as
      // "did the click do anything?" by users.
      //
      // Both the menu form (density:apply:<preset>, post-confirmation Yes)
      // and the CLI shortcut form (density:<preset>) close. The verb form
      // (`density <preset>`) was already silent.
      if (densityApplyMatch || densityMenuMatch) {
        ctx.setActiveCommand(null);
        ctx.setVerboseReturnOnEscape(null);
      }
      return true;
    }

    // Display flag toggles: set:<key> — flips the boolean. Args mode uses
    // the explicit `set:toolArgsMode:<value>` form because it's a 3-state.
    if (trimmed.startsWith('set:')) {
      const rest = trimmed.slice('set:'.length);
      // Use getVerboseDisplay so the !display.showThinkingContent flip
      // computes off the unified value (cli.json override) rather than
      // the verbose-config.json copy that may be stale.
      const display = getVerboseDisplay();
      if (rest === 'showToolReasoning') {
        setVerboseConfig({
          display: {
            ...display,
            showToolReasoning: !display.showToolReasoning,
          },
        });
        openToolMenu();
        return true;
      }
      if (rest === 'showElapsed') {
        setVerboseConfig({
          display: { ...display, showElapsed: !display.showElapsed },
        });
        openToolMenu();
        return true;
      }
      if (rest === 'showWriteDiffs:tool') {
        setVerboseConfig({
          display: { ...display, showWriteDiffs: !display.showWriteDiffs },
        });
        openToolMenu();
        return true;
      }
      if (rest === 'showThinkingContent') {
        const newVal = !display.showThinkingContent;
        setVerboseConfig({
          display: {
            ...display,
            showThinkingContent: newVal,
          },
        });
        // Also notify the ACP/Rust side so any modern-TUI surface holding
        // a React copy of `chat.showThinking` (DisplaySettingsPanel,
        // useShowThinking) picks up the new value on its next read. The
        // verbose.ts mirror has already written cli.json directly, so
        // this RPC is idempotent — Rust does locked R-M-W on the same
        // file and reads back the value we just wrote. Best-effort: a
        // failed RPC doesn't roll back the lite-side toggle.
        ctx.kiro
          .setSetting(Settings.CHAT_SHOW_THINKING, newVal)
          .catch(() => {});
        openTopMenu('thinking');
        return true;
      }
      if (rest === 'showTasks') {
        setVerboseConfig({
          display: { ...display, showTasks: !display.showTasks },
        });
        openTopMenu('tasks');
        return true;
      }
      const argMatch = rest.match(/^toolArgsMode:(off|inline|block)$/);
      if (argMatch) {
        const mode = argMatch[1] as ToolArgsMode;
        setVerboseConfig({ display: { ...display, toolArgsMode: mode } });
        openToolMenu();
        return true;
      }
      // Truncation cap setters. All four knobs share `set:<field>:<value>`
      // where value is either `null` (unlimited) or a positive integer. The
      // saved field is `null | number` — non-numeric / non-positive values
      // collapse to `null` upstream in mergeDisplay so a corrupt value can't
      // silently truncate everything to 0.
      const capMatch = rest.match(
        /^(argsMaxLines|outputMaxLines|argsMaxChars|outputMaxChars):(null|\d+)$/
      );
      if (capMatch) {
        const field = capMatch[1] as
          | 'argsMaxLines'
          | 'outputMaxLines'
          | 'argsMaxChars'
          | 'outputMaxChars';
        const raw = capMatch[2]!;
        let value: number | null;
        if (raw === 'null') {
          value = null;
        } else {
          const n = parseInt(raw, 10);
          // Non-positive values are not user-selectable from the menu, but
          // guard for them on CLI typed input.
          value = Number.isFinite(n) && n > 0 ? n : null;
        }
        setVerboseConfig({ display: { ...display, [field]: value } });
        // After committing a value from the editor, return to the truncation
        // submenu so the user sees the updated cap on the row they just
        // edited and can navigate elsewhere or pick the other cap.
        openTruncationMenu();
        return true;
      }
      const subMatch = rest.match(
        /^subagent:(pipeline|prompts|roles|deps|responses)$/
      );
      if (subMatch) {
        const key = subMatch[1] as keyof typeof display.subagent;
        const cur = display.subagent[key];
        setVerboseConfig({
          display: {
            ...display,
            subagent: { ...display.subagent, [key]: !cur },
          },
        });
        openSubagentMenu();
        return true;
      }
      // fullOutput: piggybacks on the `subagent` filter token (same gate the
      // renderer already uses for the verbose `full output:` body). Mirrors
      // the category:<name> toggle logic so behavior is consistent — when
      // filters is `['all']`, expand to the full category list before
      // dropping `subagent`; otherwise just add or remove the token.
      if (rest === 'subagent:fullOutput') {
        const curFilters = getVerboseConfig().filters;
        const isAll = curFilters.includes('all');
        const baseline = isAll
          ? Array.from(VERBOSE_CATEGORIES)
          : [...curFilters];
        const filterSet = new Set(baseline);
        if (filterSet.has('subagent')) filterSet.delete('subagent');
        else filterSet.add('subagent');
        setVerboseConfig({ filters: Array.from(filterSet) });
        openSubagentMenu();
        return true;
      }
      ctx.showAlert(`Unknown toggle: ${rest}`, 'error', 3000);
      return true;
    }

    // Multi-token subcommands: only/add/remove. The first word is the verb,
    // remaining whitespace-separated tokens are the filter list. We validate
    // tokens up front so typos surface as a warning instead of silently
    // landing in the saved config.
    const subMatch = trimmed.match(/^(only|add|remove)\b\s*(.*)$/);
    if (subMatch) {
      const verb = subMatch[1] as 'only' | 'add' | 'remove';
      const rest = (subMatch[2] ?? '').trim();
      if (!rest) {
        ctx.showAlert(
          `/verbosity ${verb} needs at least one filter token`,
          'error',
          3000
        );
        return true;
      }
      const tokens = rest.split(/\s+/);
      const { accepted, rejected, unknown } = validateTokens(tokens);
      if (accepted.length === 0) {
        ctx.showAlert(
          `No valid tokens in: ${rejected.join(', ')}`,
          'error',
          4000
        );
        return true;
      }
      const current = cfg.filters.includes('all') ? [] : [...cfg.filters];
      let nextFilters: string[];
      if (verb === 'only') {
        nextFilters = accepted;
      } else if (verb === 'add') {
        const set = new Set(current);
        for (const t of accepted) set.add(t);
        nextFilters = Array.from(set);
      } else {
        const drop = new Set(accepted);
        nextFilters = current.filter((t) => !drop.has(t));
      }
      setVerboseConfig({ filters: nextFilters });
      const tail =
        rejected.length > 0 ? ` (ignored: ${rejected.join(', ')})` : '';
      // Soft-warn on unknown tokens (typos, unrecognized categories) on the
      // remove path too — a user removing a misspelled tool name should
      // know the input didn't match anything saved.
      const warn =
        unknown.length > 0
          ? ` · warning: ${unknown.join(', ')} ${unknown.length === 1 ? `doesn't` : `don't`} match any known tool or category`
          : '';
      showStatus(`verbosity: filters updated${tail}${warn}`);
      return true;
    }

    if (trimmed.startsWith('category:')) {
      const cat = trimmed.slice('category:'.length);
      if (!VERBOSE_CATEGORIES.includes(cat as any)) {
        ctx.showAlert(`Unknown category: ${cat}`, 'error', 3000);
        return true;
      }
      const cur = getVerboseConfig();
      const isAll = cur.filters.includes('all');
      // First toggle out of "all" expands the implicit set so we can drop
      // a single category from it. Without this, removing one category
      // from "all" would silently leave the user with everything still on.
      const baseline = isAll
        ? Array.from(VERBOSE_CATEGORIES)
        : [...cur.filters];
      const filterSet = new Set(baseline);
      if (filterSet.has(cat)) {
        filterSet.delete(cat);
      } else {
        filterSet.add(cat);
      }
      const next = Array.from(filterSet);
      setVerboseConfig({ filters: next });
      // Re-open the output sub-menu so the user can keep toggling categories.
      openOutputMenu();
      return true;
    }

    ctx.showAlert(
      `Unknown /verbosity subcommand: ${trimmed}. Try /verbosity, /verbosity on|off|status|all, /verbosity density <preset>, /verbosity only|add|remove <list>.`,
      'error',
      6000
    );
    return true;
  },

  switchToPlanMode: (result, ctx) => {
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
    const enriched = enrichTurnsWithPreview(
      turns,
      ctx.getMessages() as TurnMessage[]
    );
    ctx.setShowRewindExplorer(true, enriched);
    return true;
  },
};

/** Order-insensitive equality on filter lists — paired with sameDisplay to
 *  detect which density preset is currently active. The saved filter list
 *  may have arbitrary token order, so we compare as sets. Both lists are
 *  short (a handful of tokens at most), so the O(n²) avoidance via Set is
 *  fine. */
function sameFilters(a: readonly string[], b: readonly string[]): boolean {
  if (a.length !== b.length) return false;
  if (a.length === 0) return true;
  const set = new Set(a);
  for (const t of b) {
    if (!set.has(t)) return false;
  }
  return true;
}

/** Deep-equality on display configs — used to detect which density preset
 *  (if any) is currently active. */
function sameDisplay(
  a: import('../lite/verbose.js').VerboseDisplayConfig,
  b: import('../lite/verbose.js').VerboseDisplayConfig
): boolean {
  if (
    a.showToolReasoning !== b.showToolReasoning ||
    a.toolArgsMode !== b.toolArgsMode ||
    a.showElapsed !== b.showElapsed ||
    a.showThinkingContent !== b.showThinkingContent ||
    a.showWriteDiffs !== b.showWriteDiffs ||
    a.argsMaxLines !== b.argsMaxLines ||
    a.outputMaxLines !== b.outputMaxLines ||
    a.argsMaxChars !== b.argsMaxChars ||
    a.outputMaxChars !== b.outputMaxChars
  )
    return false;
  const ks = ['pipeline', 'prompts', 'roles', 'deps', 'responses'] as const;
  for (const k of ks) {
    if (a.subagent[k] !== b.subagent[k]) return false;
  }
  return true;
}

import { formatImageLabel } from '../utils/image-label.js';
import { MessageRole } from '../stores/app-store.js';
import { kiroDark } from '../theme/kiroDark.js';
import { kiroLight } from '../theme/kiroLight.js';
import { kiroSafe } from '../theme/kiroSafe.js';
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
      workspacePaths: [process.cwd()],
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
  const effectName = commandEffects[cmdName as CommandName];
  if (effectName) {
    return effectHandlers[effectName]?.(result, ctx, cmd, args) === true;
  }
  return false;
}
