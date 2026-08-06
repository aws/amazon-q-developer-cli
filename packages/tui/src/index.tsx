#!/usr/bin/env bun
import { writeFileSync, writeSync } from 'fs';
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
  useAppStore,
} from './stores/app-store';
import { logger } from './utils/logger';
import { extractRpcErrorMessage } from './utils/error-handling';
import {
  classifyCloudError,
  cloudErrorGuidance,
} from './utils/cloud-error-classify';
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
import {
  isFullSessionId,
  resolveResumeTarget,
} from './utils/resolve-resume-target';
import { resolveAgentEngine } from './agent-engine';
import { TestModeProvider } from './test-utils/TestModeProvider';
import { parseCliArgs, buildAcpArgs } from './utils/cli-args';
import { sessionConversationsStore } from './stores/session-conversations.js';
import { pickSessionFromEntries } from './utils/session-picker';
import {
  emitCloudDetachNoticeOnce,
  hasEmittedCloudDetachNotice,
  setCloudDetachNoticePreamble,
} from './utils/cloud-detach-notice';
import {
  droppedReposFromWarnings,
  resolveSourceProviderConnection,
} from './utils/repo-attach';
import { drainConnectedProviderRepos } from './utils/cloud-repo-drain';
import { formatMissingSourceProviderGuidance } from './utils/cloud-urls';
import { Feature, features } from './features';
import type { AgentStreamEvent } from './types/agent-events';
import { isWorkflowSession, type SessionEvent } from './types/multi-session';
import { truncateToRecentTurns } from './utils/truncate-history';
import {
  readBoolSetting,
  readStringSetting,
  readOptionalStringSetting,
} from './utils/cli-settings';
import { resolvePreserveScrollback } from './utils/renderer-options';
import { UiModeSource } from './types/generated/chat-cli';
import type { UiMode } from './types/ui-mode.js';
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
  recordTuiRender,
  recordTuiWorkflowObservations,
  forceFlushMetrics,
} from './utils/tui-telemetry-observer';
import { getCliVersion } from './utils/version.js';
import {
  emitCurrentTitle,
  initTerminalTitle,
  refreshFromSession,
  resetTerminalTitle,
} from './utils/terminal-title';
import { installConsoleInterceptor } from './utils/console-interceptor';
import { connectMouseCapture } from './utils/mouse-capture.js';
import { workflowStore } from './stores/workflow-store.js';
import { WorkflowTelemetryTracker } from './utils/workflow-telemetry.js';

// Route every `console.*` call through `logger` (file-only). Must run
// before any third-party code (notably `@agentclientprotocol/sdk`)
// fires its hardcoded `console.error` and leaks to the user terminal.
installConsoleInterceptor();

function StartupReadyReporter() {
  const isInitialized = useAppStore((state) => state.isInitialized);
  const trustAllToolsRequested = useAppStore(
    (state) => state.trustAllToolsRequested
  );
  const trustAllToolsConfirmed = useAppStore(
    (state) => state.trustAllToolsConfirmed
  );
  const reported = useRef(false);

  useEffect(() => {
    if (
      reported.current ||
      !isInitialized ||
      (trustAllToolsRequested && !trustAllToolsConfirmed)
    ) {
      return;
    }

    const path = process.env.KIRO_TUI_READY_FILE;
    const token = process.env.KIRO_TUI_READY_TOKEN;
    if (!path || !token) return;

    reported.current = true;
    try {
      writeFileSync(path, token, { encoding: 'utf8', flag: 'wx', mode: 0o600 });
    } catch (error) {
      logger.warn(
        '[startup] failed to acknowledge TUI readiness',
        String(error)
      );
    }
  }, [isInitialized, trustAllToolsConfirmed, trustAllToolsRequested]);

  return null;
}

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

// On any graceful exit of a cloud session, tell the user it keeps running and
// how to reattach. Gated on the ACTUAL cloud placement (isCloudSessionActive
// is false when --cloud degraded to local), so it never misfires on a local
// session; dark-safe (always false on released builds).
const emitDetachNoticeIfCloud = (): void => {
  try {
    if (kiro?.isCloudSessionActive?.() && kiro.sessionId) {
      emitCloudDetachNoticeOnce(kiro.sessionId);
    }
  } catch {
    // Never block shutdown on the notice.
  }
};

let terminalReset = false;

// Set once the renderer exists; restores legacy keyboard reporting (Kitty
// keyboard protocol / modifyOtherKeys).
let resetKeyboardModes: (() => void) | null = null;

// Restore every terminal mode the TUI turns on: focus reporting (1004),
// bracketed paste (2004), keyboard protocol, raw mode, progress, and title.
// Synchronous and run-once, so it is safe to invoke from the 'exit' event.
const resetTerminal = () => {
  if (terminalReset) return;
  terminalReset = true;
  try {
    disableFocusTracking();
    process.stdout.write(DISABLE_BRACKETED_PASTE);
    resetKeyboardModes?.();
    process.stdin.setRawMode?.(false);
    clearTerminalProgress();
    cmuxCleanup();
    resetTerminalTitle();
  } catch {
    // stdout/stdin may already be dead (e.g. PTY closed), ignore errors
  }
};

process.on('exit', resetTerminal);

const cleanup = () => {
  resetTerminal();
  emitDetachNoticeIfCloud();
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
  // Covers the /quit + natural-drain path (which unmounts rather than routing
  // through cleanup()). Print-once + gated, so no double-print with cleanup().
  emitDetachNoticeIfCloud();
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
let initialHistoryReplayComplete = false;
const workflowTelemetryTracker = new WorkflowTelemetryTracker();
const workflowTelemetryVersion = getCliVersion();

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

  kiro.onToolsUpdate((tools, sessionTagged) => {
    logger.debug('[tui] tools update received:', tools.length, 'tools');
    appStore.getState().setToolsList(tools);
    if (sessionTagged) appStore.getState().markCloudSnapshotReceived('tools');
  });

  kiro.onMcpServersUpdate((servers) => {
    appStore.getState().updateMcpServerStatuses(servers);
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

  kiro.onModelUpdate((model) => {
    appStore.getState().setCurrentModel(model);
  });

  kiro.onKasAgentsUpdate((agents) => {
    appStore.getState().setKasAvailableAgents(agents);
    // An empty list is the cloud config-surface reset: the
    // displayed current agent belongs to the previous session, so blank the
    // chip until the sandbox reports (the load self-heal re-emits both the
    // list and the current agent).
    if (agents.length === 0) {
      appStore.getState().setCurrentAgent(null);
    }
  });

  kiro.onKasModelConfigUpdate((event) => {
    appStore.getState().handleKasModelConfigEvent(event);
  });

  kiro.onAgentUpdate((agent) => {
    const state = appStore.getState();
    // On first agent update, initialize previousAgentName so Shift+Tab always has a fallback
    if (!state.previousAgentName && agent.name !== 'kiro_planner') {
      appStore.setState({ previousAgentName: agent.name });
    }
    state.setCurrentAgent(agent);
  });

  kiro.onCompactionStatus((event) => {
    appStore.getState().handleCompactionEvent(event);
  });

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

  const questionHandler = appStore.getState().createStreamEventHandler();
  kiro.onQuestionRequest((event) => {
    questionHandler(event);
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
  initialHistoryReplayComplete = false;
  workflowTelemetryTracker.reset();

  wireUpHandlers();

  // Wire subagent list updates to store sessions
  kiro.onSubagentListUpdate((subagents: any[], pendingStages: any[] = []) => {
    const state = appStore.getState();
    subagents.forEach((sub: any) => {
      const existing = state.sessions.get(sub.sessionId);
      if (isWorkflowSession(existing)) return;

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
      if (isWorkflowSession(s)) return;
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
      if (isWorkflowSession(s) || s.status === 'pending') return;
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
  kiro.onSessionEvent((event: SessionEvent) => {
    const state = appStore.getState();
    if (event.type === 'session_terminated') {
      state.cleanupTerminatedSession(event.sessionId);
      state.updateSession(event.sessionId, {
        status: 'terminated',
        lastActivity: new Date(),
      });
    } else if (event.type === 'session_removed') {
      state.cleanupTerminatedSession(event.sessionId);
      state.removeSession(event.sessionId);
      sessionConversationsStore.getState().clearSession(event.sessionId);
      sessionHandlers.delete(event.sessionId);
    } else if (event.type === 'session_status_changed') {
      state.updateSession(event.sessionId, {
        status: event.status,
        lastActivity: new Date(),
      });
    } else if (event.type === 'session_conversation_reset') {
      sessionConversationsStore.getState().clearSession(event.sessionId);
      sessionHandlers.delete(event.sessionId);
    } else if (event.type === 'session_turn_started') {
      sessionHandlers.delete(event.sessionId);
    } else if (event.type === 'session_approvals_cancelled') {
      state.cancelSessionApprovals(event.sessionId);
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
  const sessionHandlers = new Map<string, (event: AgentStreamEvent) => void>();
  const getOrCreateHandler = (sessionId: string) => {
    if (!sessionHandlers.has(sessionId)) {
      sessionHandlers.set(
        sessionId,
        sessionConversationsStore.getState().createHandlerForSession(sessionId)
      );
    }
    return sessionHandlers.get(sessionId)!;
  };
  kiro.onMultiSessionUpdate((sessionId: string, event: AgentStreamEvent) => {
    appStore.getState().pushSessionEvent(sessionId, event);
    getOrCreateHandler(sessionId)(event);
  });
  const workflowLifecycleHandler = appStore
    .getState()
    .createStreamEventHandler();
  kiro.onWorkflowProgress((event, source) => {
    workflowStore.getState().applyEvent(event.event);
    recordTuiWorkflowObservations(
      workflowTelemetryTracker.observe(
        event.event,
        source === 'live' && initialHistoryReplayComplete
      ),
      workflowTelemetryVersion
    );
    if (source === 'live' && initialHistoryReplayComplete) {
      workflowLifecycleHandler(event);
    }
  });
  // Reset handler when user sends a message — ensures next response starts a fresh turn
  kiro.onSessionMessageSent = (sessionId: string) =>
    sessionHandlers.delete(sessionId);
  appStore.setState({
    resetSessionHandler: (sessionId: string) =>
      sessionHandlers.delete(sessionId),
  } as any);

  // Boot stages — visible above the per-MCP list while connecting. Each
  // flips loading → ready as the corresponding async step completes, so the
  // user sees real progress instead of an opaque "connecting..." line.
  appStore
    .getState()
    .setBootStage('agent_connect', 'connecting to agent', 'loading');

  // Cloud footer: surface the launch-bound repos as the location indicator
  // and pre-check them in /repo. Dark-safe: empty for non-cloud launches.
  appStore
    .getState()
    .applyRepoFooter(cliArgs.cloud ? (cliArgs.repo ?? []) : []);

  // Flag cloud sessions so cloud-only commands become visible.
  appStore.getState().setCloudSessionActive(!!cliArgs.cloud);

  appStore.getState().resetClientDisplayCaches();
  initPromise = kiro
    .initialize(agentPath, acpArgs, {
      // CLI flag > cli.json setting > undefined (let agent pick default)
      initialAgent:
        cliArgs.agent || readOptionalStringSetting('chat.defaultAgent'),
      // Pass the explicit --model flag only; the saved `chat.defaultModel` is
      // re-read fresh inside newSession so a mid-run sticky write is honored.
      initialModel: cliArgs.model,
      initialEffort: cliArgs.effort,
      // Cloud sandbox (dark-shipped): --cloud maps to a cloud-sandbox
      // execution target, sent as _meta.kiro.executionTarget on session/new.
      executionTarget: cliArgs.cloud ? { kind: 'cloud-sandbox' } : undefined,
      repos: cliArgs.repo,
      kasSubagentRoutingStore: appStore.getState().kasSubagentRouting,
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

      // A cloud session needs a connected source provider, verified before the
      // session is created and before any TUI chrome renders. When none is
      // linked the gate is the only thing on screen and bring-up parks until a
      // retry connects. Dark-safe: non-cloud sessions skip this; a missing
      // catalog is silently skipped so it never blocks.
      if (cliArgs.cloud) {
        try {
          const source = kiro.getRepoProviderSource();
          let list = await source.listSourceProviders();
          let conn = resolveSourceProviderConnection(list);
          if (list && !conn.connected) {
            if (cliArgs.noInteractive) {
              // No TUI to host the connect gate and no one to retry it, so the
              // awaited gate below would never dismiss and the process would
              // hang. Emit setup guidance and exit non-zero so scripts fail fast.
              process.stderr.write(
                `${formatMissingSourceProviderGuidance(conn.setupUrl)}\n`
              );
              process.exit(1);
            }
            appStore
              .getState()
              .setShowSourceProviderGate(true, conn.setupUrl ?? null);
            // The gate's retry re-probes via the store and dismisses itself on
            // success; block here until that happens (quit exits the process),
            // then re-read the list so enumeration below sees the connection.
            await new Promise<void>((resolve) => {
              const unsubscribe = appStore.subscribe((s) => {
                if (!s.showSourceProviderGate) {
                  unsubscribe();
                  resolve();
                }
              });
            });
            list = await source.listSourceProviders();
            conn = resolveSourceProviderConnection(list);
          }
          if (list && conn.connectedProviders.length > 0) {
            // Surface every connected provider as one checklist line, e.g.
            // "Connected to GitHub, GitLab".
            appStore
              .getState()
              .setCloudProvider(conn.connectedProviders.join(', '));
          }
          // Repo enumeration for the startup checklist ("N repositories found")
          // and the footer branch. The FULL catalog is drained (uncapped, same
          // as /repo) so the checklist count always matches /repo's All(N) —
          // but as a floating promise so a large catalog never delays
          // bring-up; the checklist row fills in when the drain lands.
          const launchRepos = (cliArgs.repo ?? [])
            .map((r) => r.trim())
            .filter(Boolean);
          const connectedProviderTypes = (list?.providers ?? [])
            .filter((p) => p.connectionStatus === 'connected')
            .map((p) => p.providerType);
          if (connectedProviderTypes.length > 0) {
            // Repo validity is NOT judged here: KAS validates each `--repo`
            // against the full catalog server-side at session/new and reports
            // dropped ones via `_meta.kiro.warnings` (consumed after
            // createSession below). This drain only feeds the count + the
            // footer branch, and floats so a large catalog never delays
            // bring-up.
            void drainConnectedProviderRepos(
              (req) => source.listSourceProviderResources(req),
              connectedProviderTypes
            )
              .then(({ resources }) => {
                // Drain EVERY connected provider to exhaustion so the checklist
                // "N repositories found" equals /repo's All(N) even when repos
                // span multiple providers or pages (already name-deduped).
                appStore.getState().setCloudRepoCount(resources.length);
                // Re-check attachment before setting the branch: this drain
                // races the bind-warning handling, which may have un-claimed
                // the repo from the footer by the time a large catalog lands.
                const footerRepo = appStore.getState().cloudRepo;
                const first =
                  footerRepo && footerRepo === launchRepos[0]
                    ? resources.find((r) => r.name === footerRepo)
                    : undefined;
                if (first?.defaultBranch) {
                  appStore.getState().setCloudBranch(first.defaultBranch);
                }
              })
              .catch(() => {
                // Count/branch are cosmetic; never surface a failure.
              });
          }
        } catch {
          // Never block cloud bring-up on the connection probe.
        } finally {
          // The provider decision is made (connected, gated, or probe failed):
          // release the connecting screen's welcome/checklist, which waited so
          // neither flashes before the gate could appear.
          appStore.getState().setCloudProviderChecked(true);
        }
      } else {
        appStore.getState().setCloudProviderChecked(true);
      }

      // Begin session tracking before the session RPC: origin (new vs resumed)
      // and a fresh model-change baseline. The explicit `--effort` launch flag
      // is boot-only state, so it is set here rather than in beginKasSession.
      // For a cloud session this runs only after the provider check above passed,
      // so the sandbox is created (and its checklist shown) only once connected.
      appStore
        .getState()
        .beginKasSession(resolvedSessionId ? 'resumed' : 'new');
      appStore.setState((s) => ({
        kas: { ...s.kas, effortExplicit: !!cliArgs.effort },
      }));
      await kiro.createSession(resolvedSessionId);
      // Cloud-only commands become visible only for a CONFIRMED cloud placement
      // (--cloud that degraded to local must not surface them).
      appStore.getState().setCloudSessionActive(kiro.isCloudSessionActive());
      // A RESUMED cloud session has no `--repo` flags to light the footer;
      // its bound repos come from the load response (`_meta.kiro.repositories`).
      // Dark-safe: a null report (local sessions, KAS builds that don't
      // report yet) leaves the footer exactly as before, while an explicit
      // report — even an empty one — is authoritative.
      if (resolvedSessionId && kiro.isCloudSessionActive()) {
        const boundRepos = kiro.getSessionRepositories();
        if (boundRepos) {
          appStore.getState().applySessionRepositories(boundRepos);
        }
      }
      // KAS validates each `--repo` against the FULL provider catalog at
      // session/new and reports dropped ones via `_meta.kiro.warnings` — the
      // authoritative bind outcome (unlike any capped client-side page scan).
      // Surface each warning and un-claim dropped repos from the footer so it
      // never shows a repo the sandbox doesn't actually have.
      const bindWarnings = kiro.getSessionNewWarnings();
      if (bindWarnings.length > 0) {
        appStore.getState().showTransientAlert({
          message: `${bindWarnings.join('; ')} — the session started without ${bindWarnings.length === 1 ? 'it' : 'them'}.`,
          status: 'warning',
          autoHideMs: 10000,
        });
        const droppedRepos = droppedReposFromWarnings(bindWarnings);
        if (droppedRepos.size > 0) {
          const kept = (cliArgs.repo ?? [])
            .map((r) => r.trim())
            .filter((r) => r && !droppedRepos.has(r));
          appStore.getState().applyRepoFooter(kept);
        }
      }
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

      // Keep history buffered until the persistent renderer can take over in order.
      const liveHandler = appStore.getState().createStreamEventHandler({
        fromHistory: pendingHistoryEvents.length > 0,
        cloudReplay: kiro.isCloudSessionActive(),
      });
      appStore.getState().setLiveStreamHandler(liveHandler);
      const enableLiveDelivery = () => {
        kiro.onHistoryEvent(() => {});
        kiro.onLiveContent(liveHandler);
      };
      if (pendingHistoryEvents.length === 0) {
        enableLiveDelivery();
      }

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
            // via lite.staticSkipBefore, so the long store costs nothing.
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
            // Keep an open replayed turn in the same renderer as its live tail.
            for (const event of events) {
              liveHandler(event);
            }
            pendingHistoryEvents = [];
            liveHandler.setHistoryReplay(false);
            enableLiveDelivery();
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
      initialHistoryReplayComplete = true;

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
      } else {
        // A resume can hit the remote session store without --cloud, so
        // backend-specific error kinds always get guidance; generic kinds
        // (network/timeout) only when --cloud proves cloud intent.
        const kind = classifyCloudError(error);
        if (
          cliArgs.cloud ||
          kind === 'version_skew' ||
          kind === 'throttling' ||
          kind === 'auth' ||
          kind === 'stream_truncated'
        ) {
          guidance = cloudErrorGuidance(kind);
        }
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

    // Cloud footer: same bound-repo indicator on the resume-picker path.
    appStore
      .getState()
      .applyRepoFooter(cliArgs.cloud ? (cliArgs.repo ?? []) : []);
    appStore.getState().setCloudSessionActive(!!cliArgs.cloud);

    await kiro.initialize(agentPath, acpArgs, {
      initialAgent:
        cliArgs.agent || readOptionalStringSetting('chat.defaultAgent'),
      // Explicit --model only; saved default is re-read inside newSession.
      initialModel: cliArgs.model,
      initialEffort: cliArgs.effort,
      // Cloud sandbox (dark-shipped): --cloud maps to a cloud-sandbox
      // execution target, sent as _meta.kiro.executionTarget on session/new.
      executionTarget: cliArgs.cloud ? { kind: 'cloud-sandbox' } : undefined,
      repos: cliArgs.repo,
      kasSubagentRoutingStore: appStore.getState().kasSubagentRouting,
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
    // Resolve the resume target against the merged listing (local + cloud
    // rows — the same data /sessions renders). This gives us two things:
    //  - Short-id support: `--list-sessions` renders 8-char ids, so a unique
    //    prefix resolves to the full id. Ambiguity keeps the input untouched
    //    and errors rather than resuming a guessed session.
    //  - Cloud auto-detect: a row marked cloud-sandbox flips the launch to
    //    cloud mode behind the scenes, so `--resume-id <cloud-id>` needs no
    //    `--cloud` — the load then starts against the remote store and the
    //    whole surface (footer, cloud commands) comes up cloud.
    // Dark-shipped: the pre-resolution shells out to `--list-sessions` (KAS
    // child spawn) before connecting, so it runs only when the remote-sandbox
    // feature is enabled — released builds keep the exact pre-existing
    // resume path (no extra spawn, no new behavior).
    const isFullId = isFullSessionId(cliArgs.resumeId);
    let ambiguousResumeId = false;
    if (
      features.isEnabled(Feature.RemoteSandbox) &&
      (!cliArgs.cloud || !isFullId)
    ) {
      const listing = await listAllSessions();
      if (listing.ok) {
        const resolution = resolveResumeTarget(
          cliArgs.resumeId,
          !!cliArgs.cloud,
          listing.sessions
        );
        if (resolution.ambiguous) {
          ambiguousResumeId = true;
          appStore
            .getState()
            .setAgentError(
              `Session id "${cliArgs.resumeId}" is ambiguous (${resolution.matchCount} matches).`,
              'Use a longer prefix or the full id from --list-sessions.'
            );
        } else {
          if (resolution.resumeId !== cliArgs.resumeId) {
            logger.info(
              `Resolved short --resume-id ${cliArgs.resumeId} to ${resolution.resumeId}`
            );
            cliArgs.resumeId = resolution.resumeId;
          }
          if (resolution.cloud && !cliArgs.cloud) {
            logger.info(
              `--resume-id ${cliArgs.resumeId} is a cloud session; enabling cloud mode`
            );
            cliArgs.cloud = true;
          }
        }
      }
    }
    // An ambiguous prefix already surfaced an error banner; skip the resume so
    // the launch starts a fresh session rather than resuming a guessed one.
    if (ambiguousResumeId) {
      // fall through with resumePickerSessionId unset
    } else if (cliArgs.cloud) {
      if (isFullSessionId(cliArgs.resumeId)) {
        // A cloud session is remote-only — ensure-session's local probes can't
        // see it. Pass the full id straight through; the connected client's
        // `session/load` resolves it against the remote store (a genuinely bad
        // id fails there with a clear error, not a misleading local not-found).
        resumePickerSessionId = cliArgs.resumeId;
      } else {
        // A short prefix can't be resolved before connecting: the pre-connect
        // listing never returns cloud rows (its one-shot KAS child isn't wired
        // to the remote store), so there is nothing to expand the prefix
        // against. Passing 8 chars to `session/load` as an exact id would load
        // the wrong session or fail opaquely — surface an error and start fresh
        // instead, mirroring the ambiguous-prefix handling above.
        appStore
          .getState()
          .setAgentError(
            `Session id "${cliArgs.resumeId}" is too short to resume a cloud session.`,
            'Use the full id from --list-sessions.'
          );
      }
    } else {
      const ensured = await ensureSession({
        sourceFormat: 'auto',
        sourceSessionId: cliArgs.resumeId,
        targetFormat: resolveAgentEngine(),
        cwd: process.cwd(),
      });
      if (ensured.ok) {
        resumePickerSessionId = ensured.sessionId;
      } else if (
        resolveAgentEngine() === 'kas' &&
        ensured.code === ErrorCode.SessionNotFound &&
        features.isEnabled(Feature.RemoteSandbox)
      ) {
        // Not in any local store — the id may name a CLOUD session (the detach
        // notice hands out ids without --cloud). Let the connected client's
        // `session/load` try the remote store; a genuinely bad id fails there
        // with a clear error instead of this misleading local not-found.
        // Feature-gated: without remote sandbox there is no remote store to
        // retry, so released builds keep the pre-existing error banner.
        resumePickerSessionId = cliArgs.resumeId;
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
        return;
      }

      if (state.pendingQuestion) {
        isExiting = true;
        appStore
          .getState()
          .setAgentError(
            'User input required but --no-interactive was specified.'
          );
        setTimeout(() => process.exit(1), 200);
        return;
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

  type UiModeSourceTag = 'envVar' | 'setting' | 'default';

  // The Rust launcher exports the stable-internal rollout decision; denied requests fall back to TUI.
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

  // Set uiMode on store (store is created before mode resolution)
  appStore.setState({ uiMode });

  // "Try Lite" nudge: recommend Lite to everyone in the rollout cohort — Lite
  // is worth suggesting even to users who've already set a default UI (they can
  // still switch or make it their default). Gated only to interactive TTYs (no
  // human to nudge otherwise) and to the lite rollout cohort (outside it /lite
  // is a no-op, so the recommendation would point at a dead command).
  const recommendLiteUi =
    !cliArgs.noInteractive && process.stdout.isTTY && liteRolloutEnabled;
  if (recommendLiteUi) {
    appStore.setState({ recommendLiteUi: true });
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
                <StartupReadyReporter />
                <AppContainer />
              </TestModeProvider>
            </AppStoreContext.Provider>
          </ThemeProvider>
        </GlyphsProvider>
      </ErrorBoundary>
    );
  }

  const rendererWideLinesEnabled = (mode: UiMode) =>
    wrapDisabled || mode === 'lite';

  // Read once: the resolver below runs from an app-store subscription that
  // fires on every batched streaming chunk, and readBoolSetting does
  // existsSync + readFileSync + JSON.parse.
  const preserveScrollbackSetting = readBoolSetting(
    Settings.CHAT_PRESERVE_SCROLLBACK,
    false
  );

  // Honored on every surface. On surfaces that draw the left status bar this
  // leaves a gap in the bar where the repaint boundary falls — accepted, since
  // keeping scrollback is worth more than an unbroken bar.
  const rendererPreserveScrollback = () =>
    resolvePreserveScrollback({ settingEnabled: preserveScrollbackSetting });

  // `wideLines` and `preserveScrollbackOnRedraw` are twinki-specific render
  // options. We type the options object explicitly so the compiler doesn't
  // require a cast.
  const renderOptions: Parameters<typeof render>[1] & {
    wideLines?: boolean;
    preserveScrollbackOnRedraw?: boolean;
  } = {
    exitOnCtrlC: false,
    patchConsole: false,
    // Install hit testing once. Terminal mouse reporting is immediately
    // disabled below and only enabled by an explicit in-app toggle.
    mouse: true,
    // Lite and wrap-disabled surfaces use wrap="overflow", where a logical
    // line can occupy multiple terminal rows. TUI -> lite switches update
    // this below so ordinary TUI sessions keep the old fast path.
    wideLines: rendererWideLinesEnabled(uiMode),
    preserveScrollbackOnRedraw: rendererPreserveScrollback(),
  };
  const instance = render(<App />, renderOptions);
  const renderTelemetryEngine: 'v2' | 'v3' =
    resolveAgentEngine() === 'kas' ? 'v3' : 'v2';
  const renderTelemetryVersion = getCliVersion();
  instance.onRenderComplete(({ durationMs, kind }) => {
    recordTuiRender(
      {
        durationMs,
        kind,
        version: renderTelemetryVersion,
        platform: process.platform,
      },
      renderTelemetryEngine
    );
  });
  // Last-resort keyboard restore for exit paths where renderer teardown is
  // skipped or throws before reaching the terminal (idempotent with it).
  // Assigned before any other post-render wiring so no exit inside this
  // function finds the safety net unset while the protocol is enabled.
  resetKeyboardModes = () => instance.resetKeyboardModes();
  const disconnectMouseCapture = connectMouseCapture(instance);
  const stopRenderer = () => {
    disconnectMouseCapture();
    instance.unmount();
  };
  // Only wideLines tracks the ui mode. Scrollback preservation no longer does:
  // it follows the setting alone, so a /tui swap cannot change it.
  let lastRendererWideLinesEnabled = rendererWideLinesEnabled(uiMode);
  appStore.subscribe((state) => {
    const enabled = rendererWideLinesEnabled(state.uiMode);
    if (enabled !== lastRendererWideLinesEnabled) {
      lastRendererWideLinesEnabled = enabled;
      instance.setWideLines(enabled);
    }
  });

  // Wire useTerminalSize to Twinki's throttled resize callback —
  // single resize path, no duplicate process.stdout listener.
  connectResizeSource(instance);

  // Expose render instance for dev metrics
  if (instance && typeof instance === 'object' && 'getMetrics' in instance) {
    (globalThis as any).__TWINKI_INSTANCE__ = instance;
  }

  // Ensure twinki unmounts cleanly on exit to prevent stale terminal writes
  appStore.setState({ onExit: stopRenderer });
  // The cloud detach notice must land on a settled terminal — unmount the
  // renderer first so the notice text can't splice into a mid-paint frame
  // (rule lines / hint fragments fusing with "Quit session ..."). Unmount is
  // idempotent, so the process-exit unmount below stays harmless.
  setCloudDetachNoticePreamble(stopRenderer);
  process.on('exit', () => {
    stopRenderer();
    try {
      if (!cliArgs.noInteractive && process.stdout.isTTY) {
        const sessionId = appStore.getState().sessionId;
        // A keep-running quit calls Kiro.close() before this handler runs,
        // nulling the session client so isCloudSessionActive() now reads false.
        // The emitted-notice flag survives close(), so honor it too — otherwise
        // the epilogue below prints "Session ended." and contradicts the detach
        // notice the keep-running path already showed.
        const cloudDetach =
          hasEmittedCloudDetachNotice() ||
          !!(kiro?.isCloudSessionActive?.() && sessionId);
        if (cloudDetach) {
          // Cloud session keeps running after we detach — show the reattach
          // notice (idempotent, so it won't double up with an earlier exit
          // path) ahead of the regular epilogue. Engine/source resolution
          // recognizes the id as cloud, so the standard resume command works.
          emitCloudDetachNoticeOnce(sessionId);
        }
        if (sessionId) {
          // A cloud session did not end (the detach notice says so); "Session
          // ended." would contradict it, so print only the reattach command on
          // that path. Local exits keep the unchanged "Session ended." epilogue.
          const epilogue = cloudDetach
            ? `\x1b[2m\nResume with: kiro-cli --resume-id ${sessionId}\n\x1b[0m`
            : `\x1b[2m\nSession ended.\nResume with: kiro-cli --resume-id ${sessionId}\n\x1b[0m`;
          writeSync(1, epilogue);
        }
      }
    } catch {
      // stdout may be closed (SIGHUP, broken pipe)
    }
  });
};

// Launch the app
startApp();
