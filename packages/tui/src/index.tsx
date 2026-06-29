#!/usr/bin/env bun
import { writeSync } from 'fs';
import { useEffect, useRef } from 'react';
import { render } from './renderer.js';
import { ErrorBoundary } from './components/ui/ErrorBoundary';
import { AppContainer } from './components/layout/AppContainer';
import { ThemeProvider } from './theme';
import { UserThemeBridge } from './theme/UserThemeBridge';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
} from './stores/app-store';
import { logger } from './utils/logger';
import { extractRpcErrorMessage } from './utils/error-handling';
import { connectResizeSource } from './hooks/useTerminalSize';
import { clearTerminalProgress } from './utils/terminal-capabilities.js';
import { cmuxCleanup } from './utils/cmux.js';
import { isGhostty } from './utils/terminal-detection.js';
import { Kiro } from './kiro';
import { ensureSession } from './utils/ensure-session-cli';
import { ErrorCode } from './types/generated/chat-internal';
import {
  isResumableSource,
  isActiveEngineSource,
  sourceFormatFor,
} from './utils/cross-engine-session-id';
import { listAllSessions } from './utils/list-all-sessions-cli';
import { resolveAgentEngine } from './agent-engine';
import { TestModeProvider } from './test-utils/TestModeProvider';
import { parseCliArgs, buildAcpArgs } from './utils/cli-args';
import { sessionConversationsStore } from './stores/session-conversations.js';
import { pickSessionFromEntries } from './utils/session-picker';
import type { AgentStreamEvent } from './types/agent-events';
import { truncateToRecentTurns } from './utils/truncate-history';
import {
  readBoolSetting,
  readStringSetting,
  readOptionalStringSetting,
} from './utils/cli-settings';
import { UiModeSource } from './types/generated/chat-cli';
import { Settings } from './constants/settings';
import { CommandHistory } from './utils/command-history';
import { GlyphsProvider } from './hooks/useGlyphs';
import { getAnnouncements } from './constants/feed.js';
import {
  getActiveAnnouncement,
  incrementShowCount,
} from './utils/feed-state.js';
import {
  ENABLE_BRACKETED_PASTE,
  DISABLE_BRACKETED_PASTE,
} from './utils/terminal-sequences';
import {
  enableFocusTracking,
  disableFocusTracking,
} from './utils/focus-tracker';
import { normalizeAtPrompt } from './utils/normalize-at-prompt';
import { isTrustGateAccepted } from './utils/trust-gate-state';
import { LITE_HISTORY_RENDER_CAP } from './components/layout/lite/static-flush';
import { startProcessHealthCollector } from './utils/process-health-collector';
import {
  recordTuiProcessHealth,
  forceFlushMetrics,
} from './utils/tui-telemetry-observer';
import {
  emitCurrentTitle,
  initTerminalTitle,
  refreshFromSession,
  resetTerminalTitle,
} from './utils/terminal-title';
import { installConsoleInterceptor } from './utils/console-interceptor';

// Route every `console.*` call through `logger` (file-only). Must run
// before any third-party code (notably `@agentclientprotocol/sdk`)
// fires its hardcoded `console.error` and leaks to the user terminal.
installConsoleInterceptor();

// Tracks which session ID has had its title synced from disk via onTurnSummary,
// so we only read the file once per session.
let titleSyncedForSession: string | undefined;

// Circuit breaker: if stdout dies (e.g. PTY closed), exit immediately.
// stdout.write() on a dead fd doesn't throw — it emits an async 'error' event.
// Without this listener, the error escalates to uncaughtException, whose handler
// writes to stdout, creating an infinite loop that leaks ~200 MB/s until OOM.
process.stdout.on('error', (err) => {
  logger.error('[tui] stdout error, exiting:', String(err));
  process.exit(1);
});

process.on('exit', (code) => {
  logger.info('[tui] exit', { code });
});

const cleanup = () => {
  try {
    disableFocusTracking();
    process.stdout.write(DISABLE_BRACKETED_PASTE);
    process.stdin.setRawMode?.(false);
    clearTerminalProgress();
    cmuxCleanup();
    resetTerminalTitle();
  } catch {
    // stdout/stdin may already be dead (e.g. PTY closed), ignore errors
  }
  process.exit(0);
};

const getAgentPath = (): string => {
  if (process.env.KIRO_MOCK_ACP === 'true') {
    return 'mock-agent-path';
  }

  if (resolveAgentEngine() === 'kas') {
    return '';
  }

  const agentPath = process.env.KIRO_CHAT_CLI_BIN;
  if (!agentPath) {
    throw new Error('KIRO_CHAT_CLI_BIN environment variable not set');
  }

  return agentPath;
};

// Shut the agent (and its MCP children) down before we leave. kiro.close()
// signals -pgid so the whole process group dies together; cleanup() resets
// the terminal and exits. Calling kiro.close() twice is safe (the second
// SIGTERM is a no-op on a dead pgid).
process.on('SIGHUP', () => {
  logger.error('[tui] SIGHUP received');
  kiro.close();
  cleanup();
});

process.on('SIGINT', () => {
  logger.error('[tui] SIGINT received');
  kiro.close();
  cleanup();
});

// SIGTERM is what `kill <pid>`, `bun --watch`, IDE restarts, and most
// supervisors send. Without this handler the TUI exits without ever
// signalling the agent subprocess, leaving acp-server.js + every MCP it
// spawned orphaned (ppid=1, never reaped).
process.on('SIGTERM', () => {
  logger.error('[tui] SIGTERM received');
  kiro.close();
  cleanup();
});

process.on('uncaughtException', (err) => {
  logger.error('[tui] uncaughtException:', err?.message || String(err));
  kiro.close();
  cleanup();
});

// beforeExit fires when the loop is about to drain naturally (e.g. all
// streams ended cleanly, no explicit exit). Last chance to nuke the agent
// before Node tears down — process.on('exit') is too late for async kills.
process.on('beforeExit', () => {
  kiro.close();
});

// Defense-in-depth: detect parent death via stdin EOF (works when stdin is piped)
process.stdin.on('end', () => {
  logger.error('[tui] stdin EOF — parent process died');
  kiro.close();
  cleanup();
});

// Pre-create kiro and store outside of React to start initialization immediately
const agentPath = getAgentPath();
const cliArgs = parseCliArgs();
const acpArgs = buildAcpArgs(cliArgs);
const kiro = new Kiro();
const appStore = createAppStore({
  kiro,
  noInteractive: cliArgs.noInteractive,
  initialInput: cliArgs.input,
  trustAllTools: cliArgs.trustAllTools,
});

// Start initialization immediately (non-blocking)
let initPromise: Promise<void> | null = null;

// Buffer history events during init so store updates don't trigger React
// re-renders that cycle Ink's stdin listener (which breaks input under Bun).
let pendingHistoryEvents: AgentStreamEvent[] = [];

const wireUpHandlers = () => {
  // Wire up history event handler so resumed sessions populate the message list.
  // Events are buffered during init and replayed in one batch afterwards.
  kiro.onHistoryEvent((event) => {
    pendingHistoryEvents.push(event);
  });

  // Wire up commands handler before initialize
  kiro.onCommandsUpdate((commands, mcpServers) => {
    appStore.getState().setSlashCommands(
      commands.map((cmd) => ({
        name: cmd.name.startsWith('/') ? cmd.name : `/${cmd.name}`,
        description: cmd.description,
        source: 'backend' as const,
        meta: cmd.meta,
      }))
    );
    // Update MCP init status from server advertisements. Seed unseen servers
    // as 'loading' so the connecting panel shows them with a spinner. The
    // agent emits commands/available repeatedly during boot — the first one
    // typically lists every server as 'loading', then later announcements
    // flip individual servers to 'running'/'failed'. Individual servers are
    // no longer rendered; only the "Loading N/M MCP server(s)" aggregate is,
    // so we track just status + startTime (per-server elapsed/error dropped).
    if (mcpServers && mcpServers.length > 0) {
      const current = appStore.getState().mcpInitStatus;
      const updated = new Map(current);
      for (const server of mcpServers) {
        const prev = updated.get(server.name);
        if (server.status === 'loading') {
          if (!prev) {
            updated.set(server.name, {
              status: 'loading',
              startTime: Date.now(),
            });
          }
        } else if (server.status === 'running') {
          if (prev?.status === 'loading') {
            updated.set(server.name, {
              status: 'ready',
              startTime: prev.startTime,
            });
          } else if (!prev) {
            updated.set(server.name, {
              status: 'ready',
              startTime: Date.now(),
            });
          }
        } else if (server.status === 'failed' || server.status === 'disabled') {
          updated.set(server.name, {
            status: 'failed',
            startTime: prev?.startTime ?? Date.now(),
          });
        }
      }
      appStore.setState({ mcpInitStatus: updated });
    }
  });

  kiro.onKasCommandsDiscovered((commands) => {
    logger.debug('[tui] KAS commands discovered:', commands.length);
    appStore.getState().setKasCommands(commands);
  });

  kiro.onToolsUpdate((tools) => {
    logger.debug('[tui] tools update received:', tools.length, 'tools');
    appStore.getState().setToolsList(tools);
  });

  // Wire up prompts handler before initialize
  kiro.onPromptsUpdate((prompts) => {
    logger.debug('[tui] prompts update received:', prompts.length, 'prompts');
    appStore.getState().setPrompts(prompts);
  });

  kiro.onSkillsUpdate((skills) => {
    logger.debug('[tui] skills update received:', skills.length, 'skills');
    appStore.getState().setSkills(skills);
  });

  kiro.onSteeringUpdate((steering) => {
    logger.debug(
      '[tui] steering update received:',
      steering.length,
      'steering docs'
    );
    appStore.getState().setSteering(steering);
  });

  // Wire up model handler before initialize
  kiro.onModelUpdate((model) => {
    appStore.getState().setCurrentModel(model);
  });

  // Wire up agent handler before initialize
  kiro.onAgentUpdate((agent) => {
    const state = appStore.getState();
    // On first agent update, initialize previousAgentName so Shift+Tab always has a fallback
    if (!state.previousAgentName && agent.name !== 'kiro_planner') {
      appStore.setState({ previousAgentName: agent.name });
    }
    state.setCurrentAgent(agent);
  });

  // Wire up compaction status handler
  kiro.onCompactionStatus((event) => {
    appStore.getState().handleCompactionEvent(event);
  });

  // Wire up turn summary handler (credits + time)
  kiro.onTurnSummary((event) => {
    appStore.getState().handleTurnSummaryEvent(event);
    // Refresh terminal title once after the first turn completes per session —
    // the backend writes the session title to disk from the first user prompt,
    // so by turn end it's guaranteed to be available.
    if (titleSyncedForSession !== kiro.sessionId && kiro.sessionId) {
      void refreshFromSession(kiro.sessionId);
      titleSyncedForSession = kiro.sessionId;
    }
  });

  // Wire up init-time notification handler (MCP failures, agent errors)
  // Create a single handler instance to avoid allocating buffering state per event.
  const initHandler = appStore.getState().createStreamEventHandler();
  kiro.onInitNotification((event) => {
    initHandler(event);
  });

  // Wire up approval requests from background sessions (e.g. /spawn).
  // These arrive outside of sendMessage() so need a persistent handler.
  const approvalHandler = appStore.getState().createStreamEventHandler();
  kiro.onApprovalRequest((event) => {
    approvalHandler(event);
  });

  // ── KAS-only wiring: spec artifact view ──
  //
  // Engine is fixed at process start (see kas-commands.ts comment),
  // so we wire this once at startup. We still call
  // `clearArtifactViewOnEngineSwitch` from any future engine-switch
  // path as defence-in-depth.
  if (resolveAgentEngine() === 'kas') {
    // Single in-flight idle timer — the store holds at most one
    // artifact-generating entry at a time, so we never need more than
    // one outstanding timer. We track the path it's keyed to so that
    // a write to a different path (the agent moved on) cleanly cancels
    // the prior timer instead of letting it fire on stale state.
    let activeTimer: { path: string; t: ReturnType<typeof setTimeout> } | null =
      null;
    const IDLE_TIMEOUT_MS = 2000;

    const clearTimer = () => {
      if (activeTimer) {
        clearTimeout(activeTimer.t);
        activeTimer = null;
      }
    };

    kiro.onArtifactWrite((match) => {
      const store = appStore.getState();
      store.notifyArtifactGenerationWrite({
        path: match.absolutePath,
        featureName: match.featureName,
        artifact: match.artifact,
      });

      // Reset the idle timer on every write. If the prior timer was
      // for a different path the store has already replaced the entry;
      // cancelling avoids a `markArtifactGenerationComplete` call on
      // the now-active entry.
      clearTimer();
      const path = match.absolutePath;
      activeTimer = {
        path,
        t: setTimeout(() => {
          activeTimer = null;
          appStore.getState().markArtifactGenerationComplete(path);
        }, IDLE_TIMEOUT_MS),
      };
    });

    // On tool completion, re-parse the on-disk file. The mid-stream
    // parses driven by `onArtifactWrite` may have observed a partially
    // written buffer (missing closing fence on a code block, half a
    // heading, etc.). The post-finish read is authoritative — by this
    // point KAS has flushed its buffer to disk.
    //
    // We still leave the idle timer in place as a fallback for write
    // tools that don't emit ToolCallFinished events, but cancel it
    // here so the "complete" transition matches the actual finish
    // event rather than waiting out the 2 s tail.
    kiro.onArtifactFinish((match) => {
      const store = appStore.getState();
      store.reparseArtifactGeneration(match.absolutePath);
      store.markArtifactGenerationComplete(match.absolutePath);

      if (activeTimer && activeTimer.path === match.absolutePath) {
        clearTimer();
      }
    });
  } else {
    // Non-KAS engine: belt-and-braces clear in case state somehow
    // ended up populated (shouldn't happen on cold start).
    appStore.getState().clearArtifactViewOnEngineSwitch();
  }
  // ── End KAS-only wiring ──
};

const startInitialization = (resumePickerSessionId?: string) => {
  if (initPromise) return initPromise;

  wireUpHandlers();

  // Wire subagent list updates to store sessions
  kiro.onSubagentListUpdate((subagents: any[], pendingStages: any[] = []) => {
    const state = appStore.getState();
    subagents.forEach((sub: any) => {
      const session = {
        id: sub.sessionId,
        name: sub.sessionName || sub.agentName,
        agentName: sub.agentName,
        status:
          sub.status?.type === 'working'
            ? ('busy' as const)
            : sub.status?.type === 'terminated'
              ? ('terminated' as const)
              : ('idle' as const),
        type: 'ephemeral' as const,
        created: sub.createdAtMs ? new Date(sub.createdAtMs) : new Date(),
        lastActivity: new Date(),
        group: sub.group,
        parentSession: sub.parentSessionId,
        role: sub.role,
      };
      const existing = state.sessions.get(sub.sessionId);
      if (existing) {
        state.updateSession(sub.sessionId, {
          name: sub.sessionName || sub.agentName,
          status: session.status,
          lastActivity: new Date(),
          group: sub.group,
          role: sub.role,
          dependsOn: sub.dependsOn ?? [],
          hasLoop: sub.hasLoop ?? false,
          loopIteration: sub.loopIteration ?? 0,
          loopMaxIterations: sub.loopMaxIterations ?? 0,
        } as any);
      } else {
        state.addSession({
          ...session,
          dependsOn: sub.dependsOn ?? [],
          hasLoop: sub.hasLoop ?? false,
          loopIteration: sub.loopIteration ?? 0,
          loopMaxIterations: sub.loopMaxIterations ?? 0,
        } as any);
      }
    });

    // Add pending stages as placeholder sessions
    pendingStages.forEach((ps: any) => {
      const pendingId = `pending:${ps.name}`;
      if (!state.sessions.get(pendingId)) {
        state.addSession({
          id: pendingId,
          name: ps.name,
          agentName: ps.agentName || ps.name,
          status: 'pending' as const,
          type: 'ephemeral' as const,
          created: new Date(),
          lastActivity: new Date(),
          group: ps.group,
          role: ps.role,
          stageInfo: { name: ps.name, role: ps.role },
          dependsOn: ps.dependsOn ?? [],
        } as any);
      } else {
        state.updateSession(pendingId, {
          dependsOn: ps.dependsOn ?? [],
        } as any);
      }
    });

    // Remove pending placeholders that are no longer pending (they got spawned as real sessions)
    const pendingNames = new Set(pendingStages.map((ps: any) => ps.name));
    state.sessions.forEach((s, id) => {
      if (s.status === 'pending' && !pendingNames.has(s.name)) {
        state.removeSession(id);
      }
    });

    // Mark busy sessions missing from list as terminated; remove old terminated sessions
    const activeIds = new Set(subagents.map((s: any) => s.sessionId));
    const activeNames = new Map<string, string>(); // name::group → sessionId (latest)
    subagents.forEach((s: any) => {
      const key = `${s.group ?? ''}::${s.sessionName || s.agentName}`;
      activeNames.set(key, s.sessionId);
    });
    // Clean up handlers for superseded sessions (same name+group but different ID)
    state.sessions.forEach((s, id) => {
      if (s.status === 'pending') return;
      if (!activeIds.has(id) && s.status === 'busy') {
        state.updateSession(id, {
          status: 'terminated' as const,
          lastActivity: new Date(),
        });
      } else if (!activeIds.has(id) && s.status === 'terminated') {
        // Only clear conversation if a newer session with same name exists
        const key = `${(s as any).group ?? ''}::${s.name}`;
        if (activeNames.has(key) && activeNames.get(key) !== id) {
          sessionConversationsStore.getState().clearSession(id);
        }
        sessionHandlers.delete(id);
      }
    });
  });

  // Wire session events
  kiro.onSessionEvent((event: any) => {
    const state = appStore.getState();
    if (event.type === 'session_terminated') {
      state.updateSession(event.sessionId, {
        status: 'terminated',
        lastActivity: new Date(),
      });
    } else if (event.type === 'session_created') {
      // Only clear conversation data for terminated sessions that share the same
      // name+group as the new session (i.e., superseded by a loop iteration).
      // This preserves output from other completed stages so users can still view them.
      if (event.session.status === 'busy') {
        const newName = event.session.name;
        const newGroup = event.session.group ?? '';
        for (const [id, s] of state.sessions) {
          if (
            s.status === 'terminated' &&
            s.name === newName &&
            (s.group ?? '') === newGroup
          ) {
            sessionConversationsStore.getState().clearSession(id);
            sessionHandlers.delete(id);
          }
        }
      }
      state.addSession(event.session);
    }
  });

  // Wire multi-session event buffer + conversation rendering
  const sessionHandlers = new Map<string, (event: any) => void>();
  const getOrCreateHandler = (sessionId: string) => {
    if (!sessionHandlers.has(sessionId)) {
      sessionHandlers.set(
        sessionId,
        sessionConversationsStore.getState().createHandlerForSession(sessionId)
      );
    }
    return sessionHandlers.get(sessionId)!;
  };
  kiro.onMultiSessionUpdate((sessionId: string, event: any) => {
    appStore.getState().pushSessionEvent(sessionId, event);
    getOrCreateHandler(sessionId)(event);
  });
  // Reset handler when user sends a message — ensures next response starts a fresh turn
  kiro.onSessionMessageSent = (sessionId: string) =>
    sessionHandlers.delete(sessionId);
  appStore.setState({
    resetSessionHandler: (sessionId: string) =>
      sessionHandlers.delete(sessionId),
  } as any);

  // Wire inbox notifications — no alert, agent reads inbox automatically
  kiro.onInboxNotification?.((notification: any) => {
    logger.info('[tui] inbox notification:', notification);
  });

  // Boot stages — visible above the per-MCP list while connecting. Each
  // flips loading → ready as the corresponding async step completes, so the
  // user sees real progress instead of an opaque "connecting..." line.
  appStore
    .getState()
    .setBootStage('agent_connect', 'connecting to agent', 'loading');

  initPromise = kiro
    .initialize(agentPath, acpArgs, {
      // CLI flag > cli.json setting > undefined (let agent pick default)
      initialAgent:
        cliArgs.agent || readOptionalStringSetting('chat.defaultAgent'),
      // Pass the explicit --model flag only; the saved `chat.defaultModel` is
      // re-read fresh inside newSession so a mid-run sticky write is honored.
      initialModel: cliArgs.model,
      hasExplicitEffort: !!cliArgs.effort,
    })
    .then(async () => {
      appStore
        .getState()
        .setBootStage('agent_connect', 'connecting to agent', 'ready');
      appStore
        .getState()
        .setBootStage('session_create', 'initializing workspace', 'loading');
      const backendSettings = kiro.settings;
      appStore.setState({
        settings: backendSettings,
        voiceAutoSubmit: backendSettings?.['voice.autoSubmit'] === true,
      });

      // Initialize announcement if greeting is enabled
      if (kiro.settings?.['chat.greeting.enabled'] !== false) {
        const active = getActiveAnnouncement(getAnnouncements());
        if (active) {
          incrementShowCount(active.id);
          appStore.getState().setAnnouncement({
            id: active.id,
            maxLines: active.maxLines,
          });
        }
      }

      // Resolve resume session ID via ACP (merged V1+V2 list from backend).
      // --resume-picker is resolved before Twinki starts (pre-passed as
      // resumePickerSessionId) because the interactive picker can't coexist
      // with Twinki's terminal input.
      let resolvedSessionId: string | undefined = resumePickerSessionId;
      if (!resolvedSessionId && cliArgs.resume) {
        // --resume: pick the most-recent session for this cwd across
        // V1 + V2 + KAS via the binary's merged --list-sessions
        // surface. Cross-engine winners are routed through
        // ensure-session before session/load so the id the active
        // engine receives is always one it owns.
        const listing = await listAllSessions();
        if (!listing.ok || listing.sessions.length === 0) {
          if (!listing.ok) {
            logger.warn(
              `Failed to list sessions for --resume: ${listing.error}`
            );
          }
          process.stderr.write(
            'No saved sessions found for this directory. Starting new session.\n'
          );
        } else {
          const activeEngine = resolveAgentEngine();
          const activeIsKas = activeEngine === 'kas';
          // The merged listing is already sorted recent-first.
          // Take the first row whose source has an implemented
          // import path into the active engine; this avoids
          // shelling out to ensure-session with a (KAS, V2) pair
          // the binary rejects.
          const winner =
            listing.sessions.find((s) =>
              isResumableSource(s.source, activeIsKas)
            ) ?? null;
          if (!winner) {
            // Every entry's source is unsupported in the active
            // engine (today: KAS-only sessions while running rust).
            // Fall through to a fresh session rather than triggering
            // a guaranteed `ensure-session` failure.
            process.stderr.write(
              'No resumable sessions found for this directory in the active engine. Starting new session.\n'
            );
          } else if (isActiveEngineSource(winner.source, activeIsKas)) {
            // Native source: the id is already one the active engine
            // owns; skip the ensure-session round-trip.
            resolvedSessionId = winner.sessionId;
          } else {
            const ensured = await ensureSession({
              sourceFormat: sourceFormatFor(winner.source),
              sourceSessionId: winner.sessionId,
              targetFormat: activeEngine,
              cwd: process.cwd(),
            });
            if (ensured.ok) {
              resolvedSessionId = ensured.sessionId;
            } else {
              logger.warn(
                `Failed to convert most-recent session for --resume: ${ensured.message}`
              );
              appStore
                .getState()
                .setAgentError(
                  `Could not resume most-recent session: ${ensured.message}`,
                  'Starting a new session instead.'
                );
            }
          }
        }
      }

      await kiro.createSession(resolvedSessionId);
      appStore
        .getState()
        .setBootStage('session_create', 'initializing workspace', 'ready');
      appStore.setState({ sessionId: kiro.sessionId ?? null });
      if (
        kiro.sessionId &&
        readStringSetting(Settings.CHAT_HISTORY_MODE, 'session') === 'session'
      ) {
        CommandHistory.getInstance().setSessionId(kiro.sessionId);
      }

      // Refresh terminal title from persisted session metadata. This catches
      // resumed sessions that already have a title from a previous conversation —
      // initTerminalTitle() at boot only had the cwd fallback since the session
      // hadn't been loaded yet.
      if (kiro.sessionId) {
        await refreshFromSession(kiro.sessionId);
        titleSyncedForSession = kiro.sessionId;
      }

      // Clear the history handler so future events (from live streaming)
      // don't get buffered.
      kiro.onHistoryEvent(() => {});

      if (pendingHistoryEvents.length > 0) {
        // Await the deferred replay so callers that chain on startInitialization()
        // (e.g. auto-submit of CLI input) don't race ahead of history.
        await new Promise<void>((resolve) => {
          setTimeout(() => {
            logger.debug(
              '[index] replaying',
              pendingHistoryEvents.length,
              'history events'
            );
            // In TUI mode, truncate to recent turns to prevent rendering
            // thousands of lines (~200ms/frame typing lag — PR #2503).
            // In lite mode, replay the full session into the store; lite
            // paints to <Static> rows and skips painting below the cap
            // via liteStaticSkipBefore, so the long store costs nothing.
            const isLite = appStore.getState().uiMode === 'lite';
            const { events, omittedTurns } = isLite
              ? { events: pendingHistoryEvents, omittedTurns: 0 }
              : truncateToRecentTurns(pendingHistoryEvents);
            if (omittedTurns > 0) {
              logger.debug(
                '[index] omitted',
                omittedTurns,
                'older turns from history replay'
              );
            }
            const handler = appStore.getState().createStreamEventHandler();
            for (const event of events) {
              handler(event);
            }
            handler.flush();
            pendingHistoryEvents = [];
            // Lite-only: clamp painted history to the most recent
            // LITE_HISTORY_RENDER_CAP messages on cold-boot --resume. The
            // store still holds the full session for context-window
            // accounting; this only suppresses Static emission below the
            // cap. No-op in TUI mode.
            appStore
              .getState()
              .setLiteStaticSkipBefore(
                Math.max(
                  0,
                  appStore.getState().messages.length - LITE_HISTORY_RENDER_CAP
                )
              );
            resolve();
          }, 0);
        });
      }

      // Mark initialization complete and drain any messages queued while initializing
      appStore.setState({ isInitialized: true });
      await appStore.getState().processQueue();
      // Process-health collector (§E, 60s). Runs for both engines: the TUI
      // samples ITSELF (process_role=tui), so it never double-counts the host's
      // pid-tree sample (process_role=host).
      const engine: 'v2' | 'v3' = resolveAgentEngine() === 'kas' ? 'v3' : 'v2';
      startProcessHealthCollector(
        (payload) => {
          kiro.sendProcessHealthMetrics(payload);
        },
        () => kiro.sessionId ?? null,
        {
          emitMetrics: (payload) => recordTuiProcessHealth(payload, engine),
          // Always wire the exit flush: it is a no-op when no metrics were
          // emitted (provider never built), and on KAS it delivers the final
          // delta window incl. the monotonic peak_rss before exit.
          flushMetrics: forceFlushMetrics,
        }
      );
    })
    .catch((error) => {
      logger.error('Failed to initialize Kiro:', error);
      // Extract the most useful error message from the RPC error.
      // ACP RequestError often carries generic `message: "Internal error"`
      // with the real cause inside `data.details` (e.g. KAS auth failures),
      // so use the shared extractor that understands that shape.
      const errorMsg = extractRpcErrorMessage(error, 'Initialization failed');
      let guidance: string | undefined;
      // Provide guidance for common init errors
      if (errorMsg.includes('active in another process')) {
        guidance =
          'Close the other session first, or start a new session without --resume.';
      }
      // Mark any in-flight boot stage as failed so the connecting list
      // doesn't sit on a spinner forever.
      const setBootStage = appStore.getState().setBootStage;
      const bp = appStore.getState().bootProgress;
      for (const [k, v] of bp.entries()) {
        if (v.status === 'loading')
          setBootStage(k, v.label, 'failed', errorMsg);
      }
      // Push into the store so React re-renders and shows the error
      appStore.getState().setAgentError(errorMsg, guidance);
    });

  return initPromise;
};

// We wrap the entire startup in an async IIFE.
const startApp = async () => {
  // Handle --resume-picker before Twinki renders: the interactive picker needs
  // raw terminal access that can't coexist with Twinki's input handling.
  // If --resume-picker is passed, list all sessions, run the picker, then
  // pass the resolved session ID into startInitialization.
  let resumePickerSessionId: string | undefined;
  if (cliArgs.resumePicker) {
    wireUpHandlers();
    await kiro.initialize(agentPath, acpArgs, {
      initialAgent:
        cliArgs.agent || readOptionalStringSetting('chat.defaultAgent'),
      // Explicit --model only; saved default is re-read inside newSession.
      initialModel: cliArgs.model,
      hasExplicitEffort: !!cliArgs.effort,
    });
    const listing = await listAllSessions();
    if (!listing.ok) {
      logger.warn(
        `Failed to list sessions for --resume-picker: ${listing.error}`
      );
      process.stderr.write(
        'Failed to list sessions for this directory. Starting new session.\n'
      );
    } else if (listing.sessions.length === 0) {
      process.stderr.write(
        'No saved sessions found for this directory. Starting new session.\n'
      );
    } else {
      const activeEngine = resolveAgentEngine();
      const activeIsKas = activeEngine === 'kas';
      // Drop entries whose source has no implemented import path into
      // the active engine (today: KAS source -> V2 target); selecting
      // one would always fail at ensure-session time.
      const resumable = listing.sessions.filter((s) =>
        isResumableSource(s.source, activeIsKas)
      );
      // Returns undefined if user pressed Esc; we fall through to a new
      // session (matching the V1 Rust picker). Ctrl+C exits the process
      // from inside the picker.
      const picked = await pickSessionFromEntries(resumable);
      if (picked) {
        if (isActiveEngineSource(picked.source, activeIsKas)) {
          resumePickerSessionId = picked.sessionId;
        } else {
          const ensured = await ensureSession({
            sourceFormat: sourceFormatFor(picked.source),
            sourceSessionId: picked.sessionId,
            targetFormat: activeEngine,
            cwd: process.cwd(),
          });
          if (ensured.ok) {
            resumePickerSessionId = ensured.sessionId;
          } else {
            logger.warn(
              `Failed to convert picked session for --resume-picker: ${ensured.message}`
            );
            process.stderr.write(
              `Could not load picked session: ${ensured.message}. Starting new session.\n`
            );
          }
        }
      }
    }
  }

  // Resolve --resume-id before render. If ensure-session can't find
  // the id we still continue to a fresh session so the user isn't
  // stuck at a hard stop, but we surface the failure both as a
  // logger.warn and as an in-app error banner so the user knows
  // their resume target wasn't honored.
  if (!resumePickerSessionId && cliArgs.resumeId) {
    const ensured = await ensureSession({
      sourceFormat: 'auto',
      sourceSessionId: cliArgs.resumeId,
      targetFormat: resolveAgentEngine(),
      cwd: process.cwd(),
    });
    if (ensured.ok) {
      resumePickerSessionId = ensured.sessionId;
    } else {
      logger.warn(
        `Failed to resolve session for --resume-id: ${ensured.message}`
      );
      const detail =
        ensured.code === ErrorCode.SessionNotFound
          ? `Failed to find session with id ${cliArgs.resumeId}`
          : `Failed to resume session ${cliArgs.resumeId}: ${ensured.message}`;
      appStore
        .getState()
        .setAgentError(detail, 'Starting a new session instead.');
    }
  }

  // Start initialization (non-blocking for the UI).
  // --resume is resolved inside startInitialization via session/list;
  // --resume-id is pre-resolved above to surface ensure-session
  // outcomes (success path or error banner) before the agent
  // handshake.
  startInitialization(resumePickerSessionId);

  // Handle non-interactive mode: bail early if no input provided
  if (cliArgs.noInteractive && !cliArgs.input) {
    process.stderr.write(
      'Error: Input must be supplied when running in non-interactive mode\n'
    );
    process.exit(1);
  }

  // In non-interactive mode, auto-accept trust-all-tools (no user to interact with the gate)
  if (cliArgs.noInteractive && cliArgs.trustAllTools) {
    appStore.getState().confirmTrustAllTools();
  }

  // Skip the trust-all-tools gate if the user previously chose "don't ask again"
  if (cliArgs.trustAllTools && isTrustGateAccepted()) {
    appStore.getState().confirmTrustAllTools();
  }

  // Non-interactive mode: auto-submit input after init, exit after turn, error on approval
  if (cliArgs.noInteractive && cliArgs.input) {
    const nonInteractiveInput = cliArgs.input;
    let hasStartedProcessing = false;
    let isExiting = false;

    // Subscribe to store changes for exit-after-turn and approval-error
    appStore.subscribe((state) => {
      if (isExiting) return;

      // Track when processing starts so we know when it ends
      if (state.isProcessing) {
        hasStartedProcessing = true;
      }

      // Error out if tool approval is requested in non-interactive mode
      if (state.pendingApproval) {
        isExiting = true;
        appStore
          .getState()
          .setAgentError(
            'Tool approval required but --no-interactive was specified.',
            'Use --trust-all-tools to automatically approve tools.'
          );
        setTimeout(() => process.exit(1), 200);
      }

      // Exit after the turn completes
      if (hasStartedProcessing && !state.isProcessing) {
        isExiting = true;
        // Give Ink a moment to flush the final render
        setTimeout(() => {
          kiro.close();
          process.exit(0);
        }, 100);
      }
    });

    // Auto-submit after initialization completes
    startInitialization().then(() => {
      if (appStore.getState().agentError) return; // Error shown via BlockingErrorAlert
      const { sendMessage, slashCommands } = appStore.getState();
      sendMessage(normalizeAtPrompt(nonInteractiveInput, slashCommands));
    });
  }

  // Interactive mode with initial input: auto-submit after init, then stay interactive (V1 behavior)
  if (!cliArgs.noInteractive && cliArgs.input) {
    const interactiveInput = cliArgs.input;
    startInitialization().then(() => {
      if (appStore.getState().agentError) return;
      const { sendMessage, slashCommands } = appStore.getState();
      sendMessage(normalizeAtPrompt(interactiveInput, slashCommands));
    });
  }

  // Some terminals (ghostty, cmux) erase the viewport on \x1b[2J without
  // preserving it in scrollback. Push content up first so it's not lost.
  if (isGhostty()) {
    process.stdout.write('\n'.repeat(process.stdout.rows || 24));
  }
  process.stdout.write('\x1b[2J\x1b[H');

  // Set process title so tmux automatic-rename shows "kiro" instead of the APC marker.
  // This doesn't override manual pane renames — only affects automatic-rename.
  process.title = 'kiro';

  // Set the terminal window title (gated by chat.terminalTitle setting)
  initTerminalTitle({
    isEnabled: () => appStore.getState().terminalTitleEnabled,
  });

  // Subscribe to setting changes so toggling chat.terminalTitle mid-session
  // takes effect immediately: enable → emit current title, disable → clear it.
  let prevTerminalTitleEnabled = appStore.getState().terminalTitleEnabled;
  appStore.subscribe((state) => {
    // No change — skip
    if (state.terminalTitleEnabled === prevTerminalTitleEnabled) {
      return;
    }
    prevTerminalTitleEnabled = state.terminalTitleEnabled;

    // User disabled the feature — clear the title bar immediately
    if (!state.terminalTitleEnabled) {
      resetTerminalTitle();
      return;
    }

    // User enabled the feature — emit the current derived title
    emitCurrentTitle();
  });

  // Resolve wrap-disabled once at startup so the renderer option and theme
  // context see the same value for the whole session. The setting lives at
  // ~/.kiro/settings/cli.json (key: chat.disableWrap).
  // KIRO_DISABLE_WRAP=1 stays supported as a dev/override escape hatch.
  const wrapDisabled =
    process.env.KIRO_DISABLE_WRAP === '1' ||
    readBoolSetting(Settings.CHAT_DISABLE_WRAP, false);

  type UiMode = 'tui' | 'lite';
  type UiModeSourceTag = 'envVar' | 'setting' | 'default';

  // Lite mode is gated on the Rust-side rollout (Feature::Lite, internal+nightly).
  // The chat-cli-v2 process exports KIRO_LITE_ROLLOUT_ENABLED=1 when the user
  // is in the cohort. Outside the cohort, lite-mode requests fall back to TUI.
  const liteRolloutEnabled = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

  // resolveUiMode returns both the chosen mode AND which input source won, so
  // telemetry can attribute "session started in lite" to env-var vs CLI vs
  // persisted setting. The persisted-default value (regardless of which source
  // won) is captured separately on the emit side so dashboards can ask "is
  // lite this user's default" without having to ignore env-driven sessions.
  function resolveUiMode(): { mode: UiMode; source: UiModeSourceTag } {
    const fromEnv = process.env.KIRO_UI_MODE;
    if (fromEnv === 'lite' || fromEnv === 'tui') {
      const mode = fromEnv === 'lite' && !liteRolloutEnabled ? 'tui' : fromEnv;
      return { mode, source: 'envVar' };
    }
    const fromSetting = readStringSetting(Settings.CHAT_UI_MODE, '');
    if (fromSetting === 'lite' || fromSetting === 'tui') {
      const mode =
        fromSetting === 'lite' && liteRolloutEnabled ? 'lite' : 'tui';
      return { mode, source: 'setting' };
    }
    return { mode: 'tui', source: 'default' };
  }

  const { mode: uiMode, source: uiModeSource } = resolveUiMode();
  // Pure user-setting passed to ThemeProvider so chrome drops only when the
  // user explicitly opted in (`chat.disableWrap` / `KIRO_DISABLE_WRAP=1`).
  // Lite mode also drops chrome, but Message/ToolUseMessage compute that at
  // render time from the live `uiMode` in the store, so a /tui ↔ /lite swap
  // restores full StatusBar chrome on new rows without baking it into theme.
  // `effectiveWrapDisabled` here only feeds twinki's `wideLines` perf hint —
  // safe to leave true for the session even after a swap to TUI mode.
  const effectiveWrapDisabled = wrapDisabled || uiMode === 'lite';

  // Set uiMode on store (store is created before mode resolution)
  appStore.setState({ uiMode });

  // First-launch UI mode picker: when nothing told us which mode to use
  // (no env var, no CLI flag, no persisted setting), block the chat UI on
  // a one-time picker so the user gets to choose their default. Skipped
  // for non-interactive launches (they don't have a human to ask) and for
  // users outside the lite rollout (they can only run TUI anyway, so the
  // picker has no real choice to offer). The resolution above already
  // returns 'tui' as the pre-pick fallback, so the TUI keeps booting in
  // the background while the gate is shown — the picker writes the same
  // chat.ui.mode setting the gate then closes on top of.
  const shouldShowFirstLaunchPicker =
    uiModeSource === 'default' &&
    !cliArgs.noInteractive &&
    process.stdout.isTTY &&
    liteRolloutEnabled;
  if (shouldShowFirstLaunchPicker) {
    appStore.setState({ firstLaunchUiModeRequested: true });
  }

  // Emit `uiModeSessionStart` exactly once per launch. `uiModeDefault` is the
  // raw persisted setting independent of which source won — that lets a
  // dashboard count "users whose default is lite" cleanly even when the
  // env var or CLI arg overrode the default for a given session.
  // sessionId is intentionally omitted: at this point the ACP session has
  // not been spawned yet, so attributing this event to a specific session id
  // would require deferring the emit. The event is per-launch, not per-turn,
  // so we accept the tradeoff and let the field stay None.
  const persistedDefault = readStringSetting(Settings.CHAT_UI_MODE, '');
  const uiModeDefault =
    persistedDefault === 'lite' || persistedDefault === 'tui'
      ? persistedDefault
      : 'unset';
  const uiModeSourceEnum: UiModeSource =
    uiModeSource === 'envVar'
      ? UiModeSource.EnvVar
      : uiModeSource === 'setting'
        ? UiModeSource.Setting
        : UiModeSource.Default;
  kiro.sendUiModeSessionStart({
    uiMode,
    uiModeSource: uiModeSourceEnum,
    uiModeDefault,
  });

  function App() {
    const appStoreRef = useRef<AppStoreApi>(appStore);

    // Enable bracketed paste + focus tracking on mount. Kitty keyboard
    // disambiguation is owned solely by twinki's ProcessTerminal, which
    // queries terminal support and keeps its `kittyProtocolActive` parser
    // flag in sync. Enabling CSI-u here too (ungated, without that flag)
    // made the terminal emit Ctrl+C as CSI-u while the parser stayed on the
    // legacy path — breaking Ctrl+C and other shortcuts (the regression that
    // got lite landing 7/9 reverted in #3227).
    useEffect(() => {
      process.stdout.write(ENABLE_BRACKETED_PASTE);
      enableFocusTracking();
      return () => {
        disableFocusTracking();
        process.stdout.write(DISABLE_BRACKETED_PASTE);
      };
    }, []);

    // Wait for initialization to complete (UI renders immediately)
    useEffect(() => {
      startInitialization();
    }, []);

    return (
      <ErrorBoundary>
        <GlyphsProvider>
          <ThemeProvider
            wrapDisabled={wrapDisabled}
            liteMode={uiMode === 'lite'}
          >
            <AppStoreContext.Provider value={appStoreRef.current}>
              <UserThemeBridge />
              <TestModeProvider>
                <AppContainer />
              </TestModeProvider>
            </AppStoreContext.Provider>
          </ThemeProvider>
        </GlyphsProvider>
      </ErrorBoundary>
    );
  }

  // `wideLines` is a twinki-specific render option. We type the options
  // object explicitly so the compiler doesn't require a cast.
  const renderOptions: Parameters<typeof render>[1] & { wideLines?: boolean } =
    {
      exitOnCtrlC: false,
      patchConsole: false,
      // Enable physical-row tracking when the user opts into disabled wrap
      // (setting `chat.disableWrap` or env `KIRO_DISABLE_WRAP=1`). Required
      // so the differential renderer places the cursor correctly for
      // soft-wrapped lines. Small per-render cost.
      wideLines: effectiveWrapDisabled,
    };
  const instance = render(<App />, renderOptions);

  // Wire useTerminalSize to Twinki's throttled resize callback —
  // single resize path, no duplicate process.stdout listener.
  connectResizeSource(instance);

  // Expose render instance for dev metrics
  if (instance && typeof instance === 'object' && 'getMetrics' in instance) {
    (globalThis as any).__TWINKI_INSTANCE__ = instance;
  }

  // Ensure twinki unmounts cleanly on exit to prevent stale terminal writes
  appStore.setState({ onExit: () => instance.unmount() });
  process.on('exit', () => {
    instance.unmount();
    try {
      if (!cliArgs.noInteractive && process.stdout.isTTY) {
        const sessionId = appStore.getState().sessionId;
        if (sessionId) {
          writeSync(
            1,
            `\x1b[2m\nSession ended.\nResume with: kiro-cli --resume-id ${sessionId}\n\x1b[0m`
          );
        }
      }
    } catch {
      // stdout may be closed (SIGHUP, broken pipe)
    }
  });
};

// Launch the app
startApp();
