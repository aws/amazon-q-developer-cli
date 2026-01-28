#!/usr/bin/env bun
import { useEffect, useRef, useState } from 'react';
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

function App() {
  const [isInitialized, setIsInitialized] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);
  const appStoreRef = useRef<AppStoreApi | null>(null);

  // Enable bracketed paste mode on mount
  useEffect(() => {
    process.stdout.write(ENABLE_BRACKETED_PASTE);
    return () => {
      process.stdout.write(DISABLE_BRACKETED_PASTE);
    };
  }, []);

  useEffect(() => {
    if (isInitialized) {
      return;
    }

    const agentPath = getAgentPath();
    const kiro = new Kiro();
    
    // Create store first so we can wire up commands handler
    const store = createAppStore({ kiro });
    appStoreRef.current = store;
    
    // Wire up commands handler before initialize
    kiro.onCommandsUpdate((commands) => {
      store.getState().setSlashCommands(
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
      store.getState().setCurrentModel(model);
    });
    
    kiro.initialize(agentPath)
      .then(() => {
        store.setState({ sessionId: kiro.sessionId ?? null });
        setIsInitialized(true);
      })
      .catch((error) => {
        logger.error('Failed to initialize Kiro:', error);
        setInitError(error.message || 'Initialization failed');
      });
  }, [appStoreRef, isInitialized, setIsInitialized]);

  if (initError) {
    return <Text color="red">Error: {initError}</Text>;
  }

  if (!appStoreRef.current) {
    return <Text>Initializing Kiro...</Text>;
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
