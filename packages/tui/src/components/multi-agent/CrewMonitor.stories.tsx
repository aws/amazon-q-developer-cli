/**
 * CrewMonitor stories — demonstrates the full session rendering pipeline.
 * Mix of grouped crew sessions and standalone /spawn sessions.
 */
import React, { useEffect, useRef } from 'react';
import { sessionConversationsStore } from '../../stores/session-conversations.js';
import { AgentEventType } from '../../types/agent-events.js';
import { CrewMonitorScreen } from '../layout/CrewMonitorScreen.js';
import { AppStoreContext, createAppStore } from '../../stores/app-store.js';
import { ThemeProvider } from '../../theme/index.js';
import { Kiro } from '../../kiro.js';

const GROUP_A = 'crew-research-trading-sys';
const GROUP_B = 'crew-implement-trading-sy';

// Mix: 2 standalone /spawn sessions + 2 crew groups
const SESSIONS = [
  // Crew A — research pipeline (parallel researchers → architect)
  {
    id: 's0',
    name: 'research-market',
    role: 'researcher',
    status: 'terminated' as const,
    group: GROUP_A,
    dependsOn: [],
  },
  {
    id: 's1',
    name: 'research-ml',
    role: 'researcher',
    status: 'terminated' as const,
    group: GROUP_A,
    dependsOn: [],
  },
  {
    id: 's2',
    name: 'research-risk',
    role: 'researcher',
    status: 'terminated' as const,
    group: GROUP_A,
    dependsOn: [],
  },
  {
    id: 's3',
    name: 'architect',
    role: 'architect',
    status: 'terminated' as const,
    group: GROUP_A,
    dependsOn: ['research-market', 'research-ml', 'research-risk'],
  },
  // Crew B — implementation pipeline
  {
    id: 's4',
    name: 'impl-data',
    role: 'implementer',
    status: 'busy' as const,
    group: GROUP_B,
    dependsOn: [],
  },
  {
    id: 's5',
    name: 'impl-risk',
    role: 'implementer',
    status: 'busy' as const,
    group: GROUP_B,
    dependsOn: [],
  },
  {
    id: 'pending:impl-exec',
    name: 'impl-exec',
    role: 'implementer',
    status: 'pending' as const,
    group: GROUP_B,
    dependsOn: ['impl-data'],
  },
  {
    id: 'pending:review',
    name: 'review',
    role: 'reviewer',
    status: 'pending' as const,
    group: GROUP_B,
    dependsOn: ['impl-data', 'impl-risk', 'impl-exec'],
  },
  // Standalone /spawn sessions (no group)
  {
    id: 'solo0',
    name: 'fix-auth-bug',
    role: 'engineer',
    status: 'busy' as const,
    group: undefined,
    dependsOn: [],
  },
  {
    id: 'solo1',
    name: 'update-readme',
    role: 'writer',
    status: 'terminated' as const,
    group: undefined,
    dependsOn: [],
  },
];

function seedConversations() {
  const store = sessionConversationsStore.getState();

  // solo0: fix-auth-bug — actively working
  const hSolo = store.createHandlerForSession('solo0');
  hSolo({
    type: AgentEventType.Content,
    id: 'x1',
    content: { type: 'text', text: 'Investigating auth token expiry issue.' },
  } as any);
  hSolo({
    type: AgentEventType.ToolCall,
    id: 'x2',
    name: 'grep',
    kind: 'grep',
    args: { pattern: 'token.*expir', path: 'src/auth' },
    locations: [],
  } as any);
  hSolo({
    type: AgentEventType.ToolCallFinished,
    id: 'x2',
    result: { status: 'success', output: '3 matches' },
  } as any);
  hSolo({
    type: AgentEventType.ToolCall,
    id: 'x3',
    name: 'fs_read',
    kind: 'read',
    args: { path: 'src/auth/token.ts' },
    locations: [{ path: 'src/auth/token.ts' }],
  } as any);
  hSolo({
    type: AgentEventType.ToolCallFinished,
    id: 'x3',
    result: { status: 'success', output: 'Read' },
  } as any);
  hSolo({
    type: AgentEventType.Content,
    id: 'x4',
    content: {
      type: 'text',
      text: 'Found the bug — refresh token not being rotated on use. Fixing now.',
    },
  } as any);
  hSolo({
    type: AgentEventType.ToolCall,
    id: 'x5',
    name: 'fs_write',
    kind: 'write',
    args: { path: 'src/auth/token.ts' },
    locations: [{ path: 'src/auth/token.ts' }],
  } as any);
  // x5 still in progress

  // research-market: completed
  const hA = store.createHandlerForSession('s0');
  hA({
    type: AgentEventType.Content,
    id: 'a1',
    content: {
      type: 'text',
      text: 'Researching real-time market data ingestion architectures.',
    },
  } as any);
  hA({
    type: AgentEventType.ToolCall,
    id: 'a2',
    name: 'web_search',
    kind: 'search',
    args: { query: 'real-time market data streaming' },
    locations: [],
  } as any);
  hA({
    type: AgentEventType.ToolCallFinished,
    id: 'a2',
    result: { status: 'success', output: '8 results' },
  } as any);
  hA({
    type: AgentEventType.Content,
    id: 'a3',
    content: {
      type: 'text',
      text: 'WebSocket feeds with FIX protocol recommended. Sub-ms latency achievable with kernel bypass.',
    },
  } as any);

  // impl-data-signals: actively working
  const hC = store.createHandlerForSession('s4');
  hC({
    type: AgentEventType.Content,
    id: 'c1',
    content: {
      type: 'text',
      text: 'Implementing data ingestion pipeline and ML signal generation.',
    },
  } as any);
  hC({
    type: AgentEventType.ToolCall,
    id: 'c2',
    name: 'fs_write',
    kind: 'write',
    args: { path: 'src/data/market-feed.ts' },
    locations: [{ path: 'src/data/market-feed.ts' }],
  } as any);
  hC({
    type: AgentEventType.ToolCallFinished,
    id: 'c2',
    result: { status: 'success', output: 'Written' },
  } as any);
  hC({
    type: AgentEventType.ToolCall,
    id: 'c3',
    name: 'execute_bash',
    kind: 'shell',
    args: { command: 'npm test -- src/signals' },
    locations: [],
  } as any);
  hC({
    type: AgentEventType.ToolCallUpdate,
    id: 'c3',
    content: {
      type: 'text',
      text: '✓ feature_extraction\n✓ ml_engine\n✓ signal_generator',
    },
  } as any);
  hC({
    type: AgentEventType.ToolCallFinished,
    id: 'c3',
    result: { status: 'success', output: '3 passed' },
  } as any);
  hC({
    type: AgentEventType.Content,
    id: 'c4',
    content: {
      type: 'text',
      text: 'All signal tests passing. Moving to WebSocket feed integration.',
    },
  } as any);
  hC({
    type: AgentEventType.ToolCall,
    id: 'c5',
    name: 'fs_write',
    kind: 'write',
    args: { path: 'src/data/ws-feed.ts' },
    locations: [{ path: 'src/data/ws-feed.ts' }],
  } as any);
  // c5 still in progress

  // impl-risk-backtest: actively working
  const hD = store.createHandlerForSession('s5');
  hD({
    type: AgentEventType.Content,
    id: 'd1',
    content: {
      type: 'text',
      text: 'Implementing risk management and backtesting framework.',
    },
  } as any);
  hD({
    type: AgentEventType.ToolCall,
    id: 'd2',
    name: 'fs_write',
    kind: 'write',
    args: { path: 'src/risk/manager.py' },
    locations: [{ path: 'src/risk/manager.py' }],
  } as any);
  hD({
    type: AgentEventType.ToolCallFinished,
    id: 'd2',
    result: { status: 'success', output: 'Written' },
  } as any);
  hD({
    type: AgentEventType.ToolCall,
    id: 'd3',
    name: 'execute_bash',
    kind: 'shell',
    args: { command: 'python -m pytest tests/risk/' },
    locations: [],
  } as any);
  hD({
    type: AgentEventType.ToolCallUpdate,
    id: 'd3',
    content: {
      type: 'text',
      text: 'test_var_calculation ... ok\ntest_position_limits ... ok\ntest_drawdown_monitor ... ',
    },
  } as any);
  // d3 still running
}

const StoryWrapper: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const storeRef = useRef(createAppStore({ kiro: new Kiro() }));

  useEffect(() => {
    // Override terminal size for storybook — process.stdout.columns is undefined in browser
    if (!process.stdout.columns) {
      (process.stdout as any).columns = 160;
      (process.stdout as any).rows = 50;
    }

    SESSIONS.forEach((s) => {
      storeRef.current.getState().addSession({
        id: s.id,
        name: s.name,
        status: s.status,
        type: 'ephemeral',
        created: new Date(),
        lastActivity: new Date(),
        role: s.role,
        group: s.group,
        dependsOn: (s as any).dependsOn ?? [],
      } as any);
    });
  }, []);

  return (
    <ThemeProvider>
      <AppStoreContext.Provider value={storeRef.current}>
        {children}
      </AppStoreContext.Provider>
    </ThemeProvider>
  );
};

export const LiveCrew = {
  component: () => {
    useEffect(() => {
      seedConversations();
    }, []);
    return (
      <StoryWrapper>
        <CrewMonitorScreen />
      </StoryWrapper>
    );
  },
  parameters: { capturesKeyboard: true },
};

export default {
  component: null,
  parameters: { layout: 'fullscreen', storyOrder: ['LiveCrew'] },
};
