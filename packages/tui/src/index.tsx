#!/usr/bin/env bun
import { useEffect, useRef } from 'react';
import { render } from 'ink';
import { Text } from 'ink';
import { ErrorBoundary } from './components/ui/ErrorBoundary';
import { AppContainer } from './components/layout/AppContainer';
import { ThemeProvider } from './theme';
import { AppStoreContext, createAppStore, type AppStoreApi } from './stores/app-store';
import { logger } from './utils/logger';
import { Kiro } from './kiro';
import { TestModeProvider } from './test-utils/TestModeProvider';

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
const kiro = new Kiro();
const appStore = createAppStore({ kiro });

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
  
  // Wire up model handler before initialize
  kiro.onModelUpdate((model) => {
    appStore.getState().setCurrentModel(model);
  });
  
  // Wire up agent handler before initialize
  kiro.onAgentUpdate((agent) => {
    appStore.getState().setCurrentAgent(agent);
  });
  
  // Wire up compaction status handler
  kiro.onCompactionStatus((event) => {
    appStore.getState().handleCompactionEvent(event);
  });
  
  initPromise = kiro.initialize(agentPath)
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
  incrementalRendering: true
});
