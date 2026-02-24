#!/usr/bin/env bun
import { useEffect, useRef } from 'react';
import { render } from 'ink';
import { Text } from 'ink';
import { ErrorBoundary } from './components/ui/ErrorBoundary';
import { AppContainer } from './components/layout/AppContainer';
import { ThemeProvider } from './theme';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
} from './stores/app-store';
import { logger } from './utils/logger';
import { Kiro } from './kiro';
import { TestModeProvider } from './test-utils/TestModeProvider';
import { parseCliArgs, buildAcpArgs } from './utils/cli-args';

// Enable bracketed paste mode escape sequences
const ENABLE_BRACKETED_PASTE = '\x1b[?2004h';
const DISABLE_BRACKETED_PASTE = '\x1b[?2004l';

const cleanup = () => {
  // Disable bracketed paste mode before exiting
  process.stdout.write(DISABLE_BRACKETED_PASTE);
  process.stdin.setRawMode?.(false);
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
  console.log('PTY closed, exiting...');
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
});

// Start initialization immediately (non-blocking)
let initPromise: Promise<void> | null = null;
let initError: string | null = null;

const startInitialization = () => {
  if (initPromise) return initPromise;

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
    logger.info('[tui] prompts update received:', prompts.length, 'prompts');
    const store = appStore.getState();
    store.setPrompts(prompts);

    // Register prompts as slash commands
    const promptCommands = prompts.map((prompt) => ({
      name: `/${prompt.name}`,
      description: prompt.description || `Prompt from ${prompt.serverName}`,
      source: 'backend' as const,
      meta: {
        type: 'prompt' as const,
        arguments: prompt.arguments,
        serverName: prompt.serverName,
      } as import('./types/commands').CommandMeta,
    }));

    // Add prompt commands to existing slash commands
    // Replace prompt commands directly in state (bypass setSlashCommands to avoid double-keep);
    appStore.setState((s) => ({
      slashCommands: [
        ...s.slashCommands.filter((c) => c.meta?.type !== 'prompt'),
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

  initPromise = kiro
    .initialize(agentPath, acpArgs)
    .then(() => {
      appStore.setState({ sessionId: kiro.sessionId ?? null });
      logger.info('Kiro initialized successfully');
    })
    .catch((error) => {
      logger.error('Failed to initialize Kiro:', error);
      initError = error.message || 'Initialization failed';
    });

  return initPromise;
};

// Start initialization immediately
startInitialization();

// Handle non-interactive mode: bail early if no input provided
if (cliArgs.noInteractive && !cliArgs.input) {
  process.stderr.write(
    'Error: Input must be supplied when running in non-interactive mode\n'
  );
  process.exit(1);
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
    appStore.getState().sendMessage(nonInteractiveInput);
  });
}

// Clear screen and move cursor to top
process.stdout.write('\x1b[2J\x1b[H');

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
      <ThemeProvider>
        <AppStoreContext.Provider value={appStoreRef.current}>
          <TestModeProvider>
            <AppContainer />
          </TestModeProvider>
        </AppStoreContext.Provider>
      </ThemeProvider>
    </ErrorBoundary>
  );
}

render(<App />, {
  exitOnCtrlC: false,
  patchConsole: false,
  incrementalRendering: false,
});
