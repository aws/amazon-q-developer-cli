#!/usr/bin/env bun
import { useEffect, useRef } from 'react';
import { Text, render } from './renderer.js';
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
import { connectResizeSource } from './hooks/useTerminalSize';
import { clearTerminalProgress } from './utils/terminal-capabilities.js';
import { Kiro } from './kiro';
import { TestModeProvider } from './test-utils/TestModeProvider';
import { parseCliArgs, buildAcpArgs } from './utils/cli-args';
import { sessionConversationsStore } from './stores/session-conversations.js';
import { pickSessionFromEntries } from './utils/session-picker';
import type { AgentStreamEvent } from './types/agent-events';
import { readBoolSetting } from './utils/cli-settings';
import { Settings } from './constants/settings';
import { getAnnouncements } from './constants/feed.js';
import {
  getActiveAnnouncement,
  incrementShowCount,
} from './utils/feed-state.js';
import {
  ENABLE_BRACKETED_PASTE,
  DISABLE_BRACKETED_PASTE,
} from './utils/terminal-sequences';
import { normalizeAtPrompt } from './utils/normalize-at-prompt';
import { isTrustGateAccepted } from './utils/trust-gate-state';

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
    process.stdout.write(DISABLE_BRACKETED_PASTE);
    process.stdin.setRawMode?.(false);
    clearTerminalProgress();
  } catch {
    // stdout/stdin may already be dead (e.g. PTY closed), ignore errors
  }
  process.exit(0);
};

const getAgentPath = (): string => {
  if (process.env.KIRO_MOCK_ACP === 'true') {
    return 'mock-agent-path';
  }

  const agentPath = process.env.KIRO_AGENT_PATH;
  if (!agentPath) {
    throw new Error('KIRO_AGENT_PATH environment variable not set');
  }

  return agentPath;
};

process.on('SIGHUP', () => {
  logger.error('[tui] SIGHUP received');
  kiro.close();
  cleanup();
});

process.on('uncaughtException', (err) => {
  logger.error('[tui] uncaughtException:', err?.message || String(err));
  kiro.close();
  cleanup();
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
let initError: string | null = null;

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
  kiro.onCommandsUpdate((commands) => {
    appStore.getState().setSlashCommands(
      commands.map((cmd) => ({
        name: cmd.name.startsWith('/') ? cmd.name : `/${cmd.name}`,
        description: cmd.description,
        source: 'backend' as const,
        meta: cmd.meta as import('./types/commands').CommandMeta | undefined,
      }))
    );
  });

  // Wire up prompts handler before initialize
  kiro.onPromptsUpdate((prompts) => {
    logger.debug('[tui] prompts update received:', prompts.length, 'prompts');
    const store = appStore.getState();
    store.setPrompts(prompts);

    // Register prompts and skills as slash commands
    const promptCommands = prompts.map((prompt) => {
      const isSkill = prompt.serverName.startsWith('skill:');
      return {
        name: `/${prompt.name}`,
        description:
          prompt.description ||
          (isSkill
            ? `Skill from ${prompt.serverName}`
            : `Prompt from ${prompt.serverName}`),
        source: 'backend' as const,
        meta: {
          type: isSkill ? 'skill' : 'prompt',
          arguments: prompt.arguments,
          serverName: prompt.serverName,
        } as import('./types/commands').CommandMeta,
      };
    });

    // Add prompt/skill commands to existing slash commands
    // Replace prompt/skill commands directly in state (bypass setSlashCommands to avoid double-keep);
    appStore.setState((s) => ({
      slashCommands: [
        ...s.slashCommands.filter(
          (c) => c.meta?.type !== 'prompt' && c.meta?.type !== 'skill'
        ),
        ...promptCommands,
      ],
    }));
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
  });

  // Wire up init-time notification handler (MCP failures, agent errors)
  // Create a single handler instance to avoid allocating buffering state per event.
  const initHandler = appStore.getState().createStreamEventHandler();
  kiro.onInitNotification((event) => {
    initHandler(event);
  });
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
        created: new Date(),
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
        } as any);
      } else {
        state.addSession({ ...session, dependsOn: sub.dependsOn ?? [] } as any);
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
    // Also clean up terminated sessions' handlers to prevent memory leaks
    state.sessions.forEach((s, id) => {
      if (s.status === 'pending') return;
      if (!activeIds.has(id) && s.status === 'busy') {
        state.updateSession(id, {
          status: 'terminated' as const,
          lastActivity: new Date(),
        });
      } else if (!activeIds.has(id) && s.status === 'terminated') {
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
      // Clear stale conversation data for terminated sessions before adding new one
      if (event.session.status === 'busy') {
        for (const [id, s] of state.sessions) {
          if (s.status === 'terminated') {
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

  initPromise = kiro
    .initialize(agentPath, acpArgs)
    .then(async () => {
      appStore.setState({ settings: kiro.settings });

      // Initialize announcement if greeting is enabled
      if (kiro.settings?.['chat.greeting.enabled'] !== false) {
        const active = getActiveAnnouncement(getAnnouncements());
        if (active) {
          incrementShowCount(active.id);
          appStore.getState().setAnnouncement({
            id: active.id,
            content: active.content,
            maxLines: active.maxLines,
          });
        }
      }

      // Resolve resume session ID via ACP (merged V1+V2 list from backend).
      // --resume-picker is resolved before Twinki starts (pre-passed as resumePickerSessionId)
      // because the interactive picker can't coexist with Twinki's terminal input.
      let resolvedSessionId: string | undefined = resumePickerSessionId;
      if (!resolvedSessionId && cliArgs.resumeId) {
        resolvedSessionId = cliArgs.resumeId;
      }
      if (!resolvedSessionId && cliArgs.resume) {
        const { sessions } = await kiro.listSessions(process.cwd());
        if (sessions.length > 0) {
          resolvedSessionId = sessions[0]!.sessionId;
        } else {
          process.stderr.write(
            'No saved sessions found for this directory. Starting new session.\n'
          );
        }
      }

      await kiro.createSession(resolvedSessionId);
      appStore.setState({ sessionId: kiro.sessionId ?? null });

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
            const handler = appStore.getState().createStreamEventHandler();
            for (const event of pendingHistoryEvents) {
              handler(event);
            }
            (handler as any).flush?.();
            pendingHistoryEvents = [];
            resolve();
          }, 0);
        });
      }

      // Mark initialization complete and drain any messages queued while initializing
      appStore.setState({ isInitialized: true });
      await appStore.getState().processQueue();
    })
    .catch((error) => {
      logger.error('Failed to initialize Kiro:', error);
      // Extract the most useful error message from the RPC error
      let errorMsg = 'Initialization failed';
      let guidance: string | undefined;
      if (typeof error === 'object' && error !== null) {
        if ('data' in error && typeof error.data === 'string' && error.data) {
          errorMsg = error.data;
        } else if (error.message && error.message !== 'Internal error') {
          errorMsg = error.message;
        }
      } else if (typeof error === 'string') {
        errorMsg = error;
      }
      // Provide guidance for common init errors
      if (errorMsg.includes('active in another process')) {
        guidance =
          'Close the other session first, or start a new session without --resume.';
      }
      initError = errorMsg;
      // Push into the store so React re-renders and shows the error
      appStore.getState().setAgentError(errorMsg, guidance);
    });

  return initPromise;
};

// We wrap the entire startup in an async IIFE.
const startApp = async () => {
  // Handle --resume-picker before Twinki renders: the interactive picker needs
  // raw terminal access that can't coexist with Twinki's input handling.
  // We start the ACP backend, list sessions, run the picker, then pass the
  // resolved ID into startInitialization.
  let resumePickerSessionId: string | undefined;
  if (cliArgs.resumePicker) {
    wireUpHandlers();
    await kiro.initialize(agentPath, acpArgs);
    const { sessions } = await kiro.listSessions(process.cwd());
    if (sessions.length > 0) {
      resumePickerSessionId = await pickSessionFromEntries(sessions);
      if (!resumePickerSessionId) {
        process.stderr.write('No session selected. Starting new session.\n');
      }
    } else {
      process.stderr.write(
        'No saved sessions found for this directory. Starting new session.\n'
      );
    }
  }

  // Start initialization (non-blocking for the UI).
  // --resume is resolved inside startInitialization via session/list.
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
      if (initError) return; // Error will be shown by the App component
      const { sendMessage, slashCommands } = appStore.getState();
      sendMessage(normalizeAtPrompt(nonInteractiveInput, slashCommands));
    });
  }

  // Interactive mode with initial input: auto-submit after init, then stay interactive (V1 behavior)
  if (!cliArgs.noInteractive && cliArgs.input) {
    const interactiveInput = cliArgs.input;
    startInitialization().then(() => {
      if (initError) return;
      const { sendMessage, slashCommands } = appStore.getState();
      sendMessage(normalizeAtPrompt(interactiveInput, slashCommands));
    });
  }

  // Some terminals (ghostty, cmux) erase the viewport on \x1b[2J without
  // preserving it in scrollback. Push content up first so it's not lost.
  if (process.env.TERM_PROGRAM === 'ghostty') {
    process.stdout.write('\n'.repeat(process.stdout.rows || 24));
  }
  process.stdout.write('\x1b[2J\x1b[H');

  // Set process title so tmux automatic-rename shows "kiro" instead of "twinki:c".
  // This doesn't override manual pane renames — only affects automatic-rename.
  process.title = 'kiro';

  // Resolve wrap-disabled once at startup so the renderer option and theme
  // context see the same value for the whole session. The setting lives at
  // ~/.kiro/settings/cli.json (key: chat.disableWrap).
  // KIRO_DISABLE_WRAP=1 stays supported as a dev/override escape hatch.
  const wrapDisabled =
    process.env.KIRO_DISABLE_WRAP === '1' ||
    readBoolSetting(Settings.CHAT_DISABLE_WRAP, false);

  function App() {
    const appStoreRef = useRef<AppStoreApi>(appStore);

    // Enable bracketed paste mode on mount
    useEffect(() => {
      process.stdout.write(ENABLE_BRACKETED_PASTE);
      return () => {
        process.stdout.write(DISABLE_BRACKETED_PASTE);
      };
    }, []);

    // Wait for initialization to complete (UI renders immediately)
    useEffect(() => {
      startInitialization();
    }, []);

    if (initError) {
      return <Text color="red">Error: {initError}</Text>;
    }

    return (
      <ErrorBoundary>
        <ThemeProvider wrapDisabled={wrapDisabled}>
          <AppStoreContext.Provider value={appStoreRef.current}>
            <UserThemeBridge />
            <TestModeProvider>
              <AppContainer />
            </TestModeProvider>
          </AppStoreContext.Provider>
        </ThemeProvider>
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
      wideLines: wrapDisabled,
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
  });
};

// Launch the app
startApp();
