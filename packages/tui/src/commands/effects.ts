/**
 * Effect handlers for slash commands registered in command-registry.ts.
 */

import type { CommandContext } from './types.js';
import { logger } from '../utils/logger.js';
import {
  armCloudScrollbackReconcile,
  cancelCloudScrollbackReconcile,
} from './cloud-scrollback-reconcile.js';
import type { CommandResult } from '../types/commands.js';
import { ModeChangeSource } from '../types/generated/chat-cli.js';
import { persistUiModeDefault } from '../utils/ui-mode-default.js';
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
import { isCommandVisibleInUiMode } from '../components/ui/command-menu-utils.js';
import { openEditorSync } from '../utils/editor.js';
import { openFileInEditor } from '../utils/editor.js';
import { extractRpcErrorMessage } from '../utils/error-handling.js';
import { runSessionLoad } from './session-load.js';
import { Kiro } from '../kiro.js';
import { getActiveGlyphs } from '../hooks/useGlyphs.js';
import { isVoiceInputAvailable } from '../features.js';
import {
  getCachedAllWorkspaceSessions,
  scanAllWorkspaceSessions,
  invalidateAllWorkspaceSessionsCache,
} from '../utils/all-workspace-sessions.js';
import { gcScan, gcEmptySessions } from '../utils/session-mutations.js';
import { getSessionSearchIndex } from '../utils/session-search.js';
import { getSessionBookmarkStore } from '../utils/session-bookmarks.js';
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
import { findConfigSubcommand } from './config-subcommands.js';
import { handleVerbosity } from './verbosity-menu.js';
import {
  getCurrentTitle,
  setUserTitle,
  clearUserTitle,
  isTerminalTitleEnabled,
} from '../utils/terminal-title.js';
import {
  getCommandEffect,
  type CommandEffectName,
} from './command-registry.js';

/** Effect handler function. Returns true if it handled its own messaging. */
export type EffectHandler = (
  result: CommandResult | null,
  ctx: CommandContext,
  cmd: AvailableCommand,
  args: string
) => boolean | void | Promise<boolean | void>;

// Symmetric same-text success confirmation: lite → scrollback row, TUI → toast.
// Reads the LIVE uiMode (getUiMode) so an awaited effect that announces after a
// surface switch routes to the current surface — unlike announceSystem, which
// reads the dispatch-time snapshot (load-bearing for callers that flip uiMode
// themselves then announce the transition, e.g. /tui).
function confirmAction(ctx: CommandContext, msg: string, ms = 3000): void {
  if (ctx.getUiMode?.() === 'lite') {
    ctx.announceSystem(msg);
  } else {
    ctx.showAlert(msg, 'success', ms);
  }
}

/**
 * Cancel any active cloud scrollback reconcile window (a new transition or
 * session switch supersedes it). Thin re-export so callers keep importing the
 * cloud-clear helpers from effects; the controller lives in
 * cloud-scrollback-reconcile (standalone, no store import).
 */
export function cancelCloudClearRewipes(): void {
  cancelCloudScrollbackReconcile();
}

/**
 * Arm the event-driven scrollback reconcile after a cloud viewport clear
 * (/clear, /chat new, /sessions new). Wipes once, then re-wipes on every
 * cloud repaint event until the startup checklist falls quiet — so a slow
 * sandbox can't outlast a fixed timer and leave pre-clear rows behind. No-op
 * off cloud: local session/new resolves before the single reset wipe, so its
 * behavior is unchanged.
 */
export function scheduleCloudClearRewipes(
  ctx: Pick<CommandContext, 'kiro' | 'bumpLiteScrollbackClear'>
): void {
  cancelCloudScrollbackReconcile();
  if (!ctx.kiro.isCloudSessionActive()) return;
  armCloudScrollbackReconcile(() => ctx.bumpLiteScrollbackClear());
}
// Fire once per session to avoid stacking deprecation rows in lite scrollback.
let themeDeprecationAnnounced = false;

/**
 * Effect handlers.
 */
const effectHandlers: Record<CommandEffectName, EffectHandler> = {
  updateModel: (result, ctx) => {
    const data = result?.data as
      | {
          model?: { id: string; name: string };
          contextUsagePercentage?: number | null;
        }
      | undefined;
    if (data?.model) {
      ctx.setCurrentModel(data.model);
      if (data.contextUsagePercentage !== undefined) {
        ctx.setContextUsage(data.contextUsagePercentage);
      }
      // Lite has no transient toast; emit a System row for scrollback visibility.
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
      const { exitCode, error } = openFileInEditor(filePath);

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

      const uiMode = ctx.getUiMode?.() === 'lite' ? 'lite' : 'tui';
      const localHelpEntries = ctx.slashCommands
        .filter(isLocalHostCommand)
        .filter((c) => isCommandVisibleInUiMode(c, uiMode))
        .map((c) => ({
          name: c.name,
          description: c.description,
          usage: c.name,
        }));
      const backendHelpEntries = data.commands.filter(
        (command) => command.name !== '/voice' || isVoiceInputAvailable()
      );
      const allCommands = [...backendHelpEntries, ...localHelpEntries].sort(
        (a, b) => a.name.localeCompare(b.name)
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
    // Lite mode: do NOT emit CSI 2J/3J (destroys terminal scrollback).
    // Backend already cleared context; dispatcher shows confirmation alert.
    if (data?.sessionId) {
      // Preserve the current agent across /clear — the user expects to stay
      // on the same agent, just with a fresh conversation.
      const previousAgent = ctx.currentAgent;
      // A cleared cloud session has no prior history, even when the replaced session was resumed.
      if (ctx.cloudSessionActive) ctx.beginKasSession('new');
      ctx.clearUIState();
      // Cloud only: the new sandbox's own pushes repopulate; a local
      // /clear's caches were already replaced by the create-RPC pushes.
      if (ctx.kiro.isCloudSessionActive()) ctx.resetClientDisplayCaches();
      ctx.resetMessages();
      // The repo footer describes the replaced session's sandbox; left in
      // place it would also get stashed under the new session's id and leak
      // back on every later switch to it.
      if (ctx.cloudSessionActive) {
        ctx.resetCloudSessionScope();
        const boundRepos = ctx.kiro.getSessionRepositories();
        if (boundRepos && boundRepos.length > 0) {
          ctx.applyRepoFooter(
            boundRepos.map((r) => r.name),
            boundRepos[0]?.branch ?? null
          );
        }
      }
      // Cloud: session/new provisions a sandbox, and the startup checklist
      // keeps re-rendering for seconds after the wipe above. Those repaints
      // interleave with the scrollback reset and can leave pre-clear rows
      // visible in the viewport (local session/new resolves fast enough that
      // the single wipe always lands last). Re-issue the wipe after the
      // transition settles so the final repaint holds only the fresh session.
      // Both consumers of the token treat a bump idempotently, so extra bumps
      // on an already-clean screen are harmless repaints. Cancel any wipes a
      // previous /clear still has pending so only one schedule is ever live.
      scheduleCloudClearRewipes(ctx);
      ctx.setSessionId(data.sessionId);
      if (data.currentModel) ctx.setCurrentModel(data.currentModel);
      if (previousAgent) {
        // Re-apply the previous agent to the new session
        ctx.kiro.setConfigOption('mode', previousAgent.name).catch(() => {
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
    // Cloud sessions keep running after detach, so /quit asks keep-running vs
    // turn-off. Local /quit is unchanged; on released builds the cloud path is
    // unreachable (no cloud-sandbox cap).
    if (ctx.kiro.isCloudSessionActive()) {
      ctx.setShowCloudQuitPrompt(true);
      return true;
    }
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
        confirmAction(ctx, 'Switched to main chat', 2000);
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

    const glyphs = getActiveGlyphs();

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
          description: `${s.status}${s.role ? ` ${glyphs.smallDot} ${s.role}` : ''}${s.group ? ` ${glyphs.smallDot} ${s.group}` : ''}`,
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

      // KAS has no manual ephemeral-spawn RPC: its spawnSession is a stub that
      // returns an empty sessionId. Without this guard the effect would build
      // a bogus `session-` display name, register a dead id:'' session (which
      // then clutters /switch with a phantom row that opens a broken
      // session-view), and announce a misleading "Spawned session-: …". Show a
      // clean "not supported" message instead and bail before touching state.
      if (!sessionId) {
        ctx.showAlert('/spawn is not supported in KAS mode', 'error', 3000);
        return;
      }

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

      confirmAction(
        ctx,
        `Spawned ${displayName}: ${task.slice(0, 40)}${task.length > 40 ? '…' : ''}`
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
    // Cloud sessions: every /spec form reads the LOCAL .kiro/specs tree,
    // which is not the sandbox filesystem the session runs on — refuse up
    // front instead of showing stale local specs. The env override is a
    // test-only seam (inert unless exactly '1') that lets the cloud-parity
    // E2E exercise the flows behind the gate without changing what users
    // see.
    if (
      ctx.cloudSessionActive &&
      process.env.KIRO_TEST_SPEC_CLOUD_PARITY !== '1'
    ) {
      ctx.showAlert(
        '/spec is not available for a cloud session yet.',
        'error',
        5000
      );
      return true;
    }
    const trimmed = args.trim();
    const workspaceRoot = process.cwd();

    // No-args path: delegate to the shared `openSpecView` helper
    // which handles both the empty-args feature picker and the
    // `<name> [artifact]` direct-open path.
    if (!trimmed) {
      return openSpecView(ctx, cmd, workspaceRoot, '');
    }

    // /spec new <name> — switch to spec mode, then arm a description step so
    // nothing reaches the agent until the user says what the spec covers.
    if (/^new(\s|$)/.test(trimmed)) {
      const name = trimmed.slice(3).trim();
      if (!name) {
        ctx.showAlert('Usage: /spec new <feature-name>', 'error', 4000);
        return true;
      }
      try {
        await ctx.kiro.setConfigOption('mode', 'spec');
      } catch (err) {
        ctx.showAlert(
          extractRpcErrorMessage(err, 'Failed to switch to spec mode'),
          'error',
          5000
        );
        return true;
      }
      ctx.setCurrentAgent({ name: 'spec' });
      ctx.setPendingSpecDescription({ featureName: name });
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
      await ctx.runSpecTasks(feature.featureName, false);
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
          await ctx.kiro.setConfigOption('mode', 'spec');
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

    confirmAction(ctx, 'Copied to clipboard');
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
    const msg =
      sessionId !== 'none'
        ? `Session ID: ${sessionId}\nResume with: kiro-cli --resume-id ${sessionId}`
        : 'Session ID: none';
    ctx.announceSystem(msg, true, 10000);
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

  openSessionDashboard: (_result, ctx, _cmd, args) => {
    // KAS-only — the dashboard + session stores are a V3 surface.
    if (ctx.agentEngine !== 'kas') {
      ctx.showAlert(
        'Session dashboard is available on the V3 (KAS) engine only',
        'warning',
        3000
      );
      return;
    }
    const sub = (args ?? '').trim().toLowerCase();
    // `/sessions rename <name>` — names the CURRENT session, the moment you
    // usually realize a conversation is worth keeping. Case is preserved
    // from the raw args; an empty name reports usage rather than clearing.
    if (sub === 'rename' || sub.startsWith('rename ')) {
      const name = (args ?? '').trim().slice('rename'.length).trim();
      const sessionId = ctx.kiro.sessionId;
      if (!sessionId) {
        ctx.showAlert('No active session to rename', 'warning', 3000);
        return;
      }
      if (!name) {
        ctx.showAlert('Usage: /sessions rename <name>', 'warning', 3000);
        return;
      }
      const saved = getSessionBookmarkStore().setTitle(sessionId, name);
      if (!saved.ok) {
        ctx.showAlert(
          `Session rename was not saved (${saved.reason})`,
          'error',
          5000
        );
        return;
      }
      ctx.kiro
        .renameSessionById(sessionId, name, {
          source: ctx.kiro.isCloudSessionActive() ? 'remote' : 'local',
        })
        .catch(() => {
          logger.debug('[sessions] rename RPC failed for', sessionId);
        });
      ctx.showAlert(`Session renamed to "${name}"`, 'success', 3000);
      return;
    }
    // `/sessions clean` — reports the plan; `/sessions clean --yes` executes.
    // Marked (bookmarked/tagged/archived), active, locked and recent
    // sessions are exempt.
    if (sub === 'clean' || sub === 'gc' || sub.startsWith('clean ')) {
      const confirmed = /(^|\s)(--yes|-y|confirm)(\s|$)/.test(sub);
      const store = getSessionBookmarkStore();
      void (async () => {
        const scan = await gcScan(
          ctx.kiro.sessionId ?? null,
          new Set(store.allUserTouched())
        );
        if (scan.candidates.length === 0) {
          ctx.showAlert('No empty sessions to clean', 'success', 3000);
          return;
        }
        const n = scan.candidates.length;
        const plural = n === 1 ? '' : 's';
        if (!confirmed) {
          const s = scan.skipped;
          const exempt =
            `${s.active} active, ${s.locked} in use, ` +
            `${s.recent} recently active, ${s.userTouched} marked, ` +
            `${s.hasParent} derived`;
          ctx.showAlert(
            `${n} empty session${plural} would be deleted permanently ` +
              `(${exempt} exempt). This cannot be undone — ` +
              `run "/sessions clean --yes" to proceed.`,
            'warning',
            12000
          );
          return;
        }
        const result = await gcEmptySessions(
          scan.candidates,
          ctx.kiro.sessionId ?? null,
          undefined,
          (id) => ctx.kiro.deleteSessionById(id)
        );
        invalidateAllWorkspaceSessionsCache();
        void scanAllWorkspaceSessions();
        // Deletions take effect in search at once.
        const index = getSessionSearchIndex();
        for (const c of scan.candidates) {
          index.update(c.sessionId, c.store === 'kas' ? 'v3' : 'v2');
        }
        const notes = [
          result.failed > 0 ? `${result.failed} failed` : null,
          result.stale > 0 ? `${result.stale} no longer empty` : null,
        ].filter(Boolean);
        ctx.showAlert(
          `Deleted ${result.deleted} empty session${result.deleted === 1 ? '' : 's'}` +
            (notes.length > 0 ? ` (${notes.join(', ')})` : ''),
          'success',
          4000
        );
      })();
      return;
    }
    // Bare `/sessions` — open the full-screen dashboard.
    ctx.setShowSessionDashboard(true, getCachedAllWorkspaceSessions(), 'slash');
    process.stdout.write('\x1b[?1049h'); // enter alt screen before re-render
    ctx.setMode('session-dashboard');
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

    // Selection-menu command shape used by every /theme submenu.
    const themeSelection = {
      ...themeCmd,
      meta: {
        ...themeCmd.meta,
        inputType: 'selection' as const,
        searchable: false,
      },
    };
    // The three custom-category rows (prompt / response / diff) with their
    // current preset labels. Shared by `custom` and the apply-return path.
    const customCategoryOptions = (p: typeof prefs) => [
      {
        value: 'prompt',
        label: 'Prompt style',
        description: getPromptPreset(p.promptPreset)?.label ?? 'Default',
      },
      {
        value: 'response',
        label: 'Response text color',
        description: getResponsePreset(p.responsePreset)?.label ?? 'Default',
      },
      {
        value: 'diff',
        label: 'Code diff colors',
        description: getDiffPreset(p.diffPreset)?.label ?? 'Default',
      },
    ];
    // Per-category data driving the preset submenu + apply. `apply` writes the
    // category's color slot via the correct setUserColors arg position.
    const CATEGORY = {
      prompt: {
        presets: promptPresets,
        active: prefs.promptPreset ?? 'default',
        get: getPromptPreset,
        label: 'Prompt style',
        apply: (preset: (typeof promptPresets)[number]) =>
          ctx.setUserColors(
            { text: preset.textColor, bg: preset.bgColor },
            undefined,
            undefined
          ),
        setPref: (pr: typeof prefs, id: string | undefined) => {
          pr.promptPreset = id;
        },
      },
      response: {
        presets: responsePresets,
        active: prefs.responsePreset ?? 'default',
        get: getResponsePreset,
        label: 'Response color',
        apply: (preset: (typeof responsePresets)[number]) =>
          ctx.setUserColors(undefined, preset.textColor, undefined),
        setPref: (pr: typeof prefs, id: string | undefined) => {
          pr.responsePreset = id;
        },
      },
      diff: {
        presets: diffPresets,
        active: prefs.diffPreset ?? 'default',
        get: getDiffPreset,
        label: 'Diff colors',
        apply: (preset: (typeof diffPresets)[number]) =>
          ctx.setUserColors(undefined, undefined, preset),
        setPref: (pr: typeof prefs, id: string | undefined) => {
          pr.diffPreset = id;
        },
      },
    } as const;
    type ThemeCategory = keyof typeof CATEGORY;

    // Open a prompt/response/diff preset submenu (ESC → custom menu).
    const openPresetSubmenu = (category: ThemeCategory) => {
      const c = CATEGORY[category];
      ctx.setThemePreview(
        buildCurrentPreview(prefs, fallbackDiff, kiroSafe.colors.brand)
      );
      ctx.setThemeReturnOnEscape('custom');
      ctx.setActiveCommand({
        command: themeSelection,
        options: c.presets.map((p) => ({
          value: `${category}:${p.id}`,
          label: p.label,
          description: p.id === c.active ? '[active]' : '',
        })),
      });
    };

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

    // /theme custom — prompt vs response vs diff selection (ESC → bare /theme).
    if (args === 'custom') {
      ctx.setThemePreview(
        buildCurrentPreview(prefs, fallbackDiff, kiroSafe.colors.brand)
      );
      ctx.setThemeReturnOnEscape('');
      ctx.setActiveCommand({
        command: themeSelection,
        options: customCategoryOptions(prefs),
      });
      return true;
    }

    // /theme prompt|response|diff — open the category's preset submenu.
    if (args === 'prompt' || args === 'response' || args === 'diff') {
      openPresetSubmenu(args);
      return true;
    }

    // /theme <category>:<id> — apply a custom selection, then re-open custom.
    if (
      args.startsWith('prompt:') ||
      args.startsWith('response:') ||
      args.startsWith('diff:')
    ) {
      const colonIdx = args.indexOf(':');
      const category = args.slice(0, colonIdx) as ThemeCategory;
      const presetId = args.slice(colonIdx + 1);
      const c = CATEGORY[category];

      const preset = c.get(presetId);
      if (!preset) {
        ctx.showAlert(`Unknown ${category} preset: ${presetId}`, 'error', 3000);
        return true;
      }
      const updatedPrefs = { ...prefs };
      c.setPref(updatedPrefs, preset.id === 'default' ? undefined : preset.id);

      c.apply(preset as any);
      const saved = saveUserThemePrefs(updatedPrefs);
      ctx.showAlert(
        saved
          ? `${c.label} set to ${preset.label}`
          : `${c.label} applied but failed to save`,
        saved ? 'success' : 'error',
        3000
      );

      // Re-open the custom menu with the updated preview (ESC → bare /theme).
      ctx.setThemePreview(
        buildCurrentPreview(updatedPrefs, fallbackDiff, kiroSafe.colors.brand)
      );
      ctx.setThemeReturnOnEscape('');
      ctx.setActiveCommand({
        command: themeSelection,
        options: customCategoryOptions(updatedPrefs),
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
      command: themeSelection,
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
        const handler = effectHandlers[name as CommandEffectName];
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

  /**
   * Open the /config overlay or route `/config <category>` (cloud_config
   * rollout only — the command isn't registered otherwise).
   *
   * Subcommands and their routing logic live in ./config-subcommands.ts,
   * mirroring /settings (settings-subcommands.ts). To add a new one, add an
   * entry there — no changes needed here. `mcp` and `hooks` do not get
   * config sub-pages: they dispatch to the same view as /mcp and /hooks so
   * each category has exactly one page.
   */
  showConfigMenu: (_result, ctx, _cmd, args) => {
    const token = args.trim().split(/\s+/)[0] ?? '';
    if (!token) {
      ctx.setShowConfigPanel(true);
      return true;
    }
    const sub = findConfigSubcommand(token);
    if (!sub) {
      ctx.showAlert(`Unknown config category: ${token}`, 'error', 3000);
      return true;
    }
    // openMcp's V2 branch awaits an RPC — surface a failure as an alert
    // rather than an unhandled rejection.
    void Promise.resolve(sub.handle({ ctx })).catch((error) => {
      const message =
        error instanceof Error ? error.message : 'Config command failed';
      ctx.showAlert(message, 'error', 3000);
    });
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
    // The launcher-owned rollout flag also guards direct /lite execution.
    if (process.env.KIRO_LITE_ROLLOUT_ENABLED !== '1') {
      ctx.announceSystem('Lite mode is not available in this build');
      return;
    }
    const fromMode = ctx.getUiMode?.() ?? 'tui';
    if (fromMode === 'lite') {
      ctx.announceSystem('Already in the Lite UI');
      return;
    }
    // Cross-mode swaps re-render existing messages in the destination layout.
    ctx.setUiMode?.('lite', '[EXPERIMENTAL] Switched to Lite UI');
    persistUiModeDefault('lite', ctx.kiro);
    ctx.kiro.sendUiModeChanged({
      from: fromMode,
      to: 'lite',
      source: ModeChangeSource.SlashCommand,
      sessionId: ctx.kiro.sessionId,
    });
  },

  switchToTui: (_result, ctx) => {
    if (ctx.getUiMode?.() !== 'lite') {
      ctx.announceSystem('Already in the TUI');
      return;
    }
    ctx.setUiMode?.('tui', 'Switched to TUI mode');
    persistUiModeDefault('tui', ctx.kiro);
    ctx.kiro.sendUiModeChanged({
      from: 'lite',
      to: 'tui',
      source: ModeChangeSource.SlashCommand,
      sessionId: ctx.kiro.sessionId,
    });
  },

  /** /verbosity implementation lives in ./verbosity-menu.ts. */
  verbosityConfig: handleVerbosity,

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

/** Every document `/spec view` can open, for validating an explicit argument. */
const VIEWABLE_DOCUMENTS: readonly ArtifactKind[] = [
  'requirements',
  'design',
  'tasks',
  'bugfix',
] as const;

function isViewableDocument(name: string): name is ArtifactKind {
  return (VIEWABLE_DOCUMENTS as readonly string[]).includes(name);
}

/**
 * Pick the most-recently-modified viewable document under
 * `.kiro/specs/<feature>/`. Returns null when the feature has none of them yet.
 *
 * Used for `/spec view <feature>` (no explicit artifact arg) so the user
 * lands on the most-active document by default.
 */
function pickMostRecentArtifact(
  workspaceRoot: string,
  featureName: string
): ArtifactKind | null {
  let best: { kind: ArtifactKind; mtime: number } | null = null;
  for (const kind of VIEWABLE_DOCUMENTS) {
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
    if (!isViewableDocument(explicitArtifact)) {
      ctx.showAlert(
        `Unknown artifact "${explicitArtifact}". Use one of: ${VIEWABLE_DOCUMENTS.join(', ')}.`,
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
        `No spec documents in .kiro/specs/${name}/ yet.`,
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
  sendMessage: (
    content: string,
    images?: Array<{ base64: string; mimeType: string }>,
    displayContent?: string
  ) => Promise<void> | void;
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
    await deps.kiro.setConfigOption('mode', 'spec');
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

export interface SendRevisionDeps extends ResumeSpecDeps {
  /**
   * Whether the agent can take a message. Re-checked after the mode switch,
   * which awaits and so can straddle a turn starting.
   */
  isBusy: () => boolean;
}

/**
 * Send comments staged against a spec document as an ordinary prompt.
 *
 * A checkpoint sends its comments by answering the question it is waiting on.
 * Outside one — `/spec view` can be opened at any time — there is no question to
 * answer, so the composed request goes as a message. It stands alone: it names
 * the document and quotes the lines each comment annotates.
 *
 * The transcript shows `summary` instead, because the request is written for the
 * agent: reading a wall of tagged quotes back is no way to see what you sent.
 *
 * Returns whether the agent actually received the request. A false means the
 * caller must keep the comments staged: they are hand-typed, and a send that was
 * accepted-but-transformed is as lossy as one that threw.
 */
export async function sendSpecRevision(
  deps: SendRevisionDeps,
  request: string,
  summary: string,
  onSent?: () => void
): Promise<boolean> {
  if (deps.isBusy()) {
    deps.showAlert(
      'The agent is busy — comments are still staged, press S again when it finishes',
      'warning',
      5000
    );
    return false;
  }
  try {
    await deps.kiro.setConfigOption('mode', 'spec');
  } catch (err) {
    deps.showAlert(
      extractRpcErrorMessage(err, 'Failed to switch to spec mode'),
      'error',
      5000
    );
    return false;
  }
  // Both halves of "we are in spec mode" apply together so a refusal below
  // cannot leave the backend switched while the transcript names the old agent.
  deps.setCurrentAgent({ name: 'spec' });
  // Re-check: the await above can straddle a turn starting.
  if (deps.isBusy()) {
    deps.showAlert(
      'The agent is busy — comments are still staged, press S again when it finishes',
      'warning',
      5000
    );
    return false;
  }
  // Clear now: the re-check just confirmed sendMessage will dispatch (not
  // queue), and once it does the comments are part of the outgoing turn. Waiting
  // for the full stream to end would leave them visible to a mid-turn checkpoint.
  onSent?.();
  await deps.sendMessage(request, undefined, summary);
  return true;
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
  const effectName = getCommandEffect(cmdName);
  if (effectName) {
    return effectHandlers[effectName]?.(result, ctx, cmd, args) === true;
  }
  return false;
}
