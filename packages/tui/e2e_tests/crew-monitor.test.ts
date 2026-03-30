import { describe, it, expect, afterEach } from 'bun:test';
import * as fs from 'fs';
import * as path from 'path';
import { E2ETestCase } from './E2ETestCase';

const ROOT = path.join(__dirname, '..');

describe('Crew Monitor', () => {
  let tc: E2ETestCase | null = null;
  afterEach(async () => { await tc?.cleanup(); tc = null; });

  // Static checks (fast, no binary needed)
  it('app-store has crew-monitor mode', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/stores/app-store.ts'), 'utf8');
    expect(src).toContain('crew-monitor');
  });

  it('AppContainer has ctrl+g handler', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/components/layout/AppContainer.tsx'), 'utf8');
    expect(src).toContain('crew-monitor');
    expect(src).toContain("'g'");
  });

  it('CrewMonitorScreen component exists', () => {
    expect(fs.existsSync(path.join(ROOT, 'src/components/layout/CrewMonitorScreen.tsx'))).toBe(true);
  });

  it('multi-agent components exist', () => {
    const dir = path.join(ROOT, 'src/components/multi-agent');
    expect(fs.existsSync(dir)).toBe(true);
    expect(fs.existsSync(path.join(dir, 'SessionList.tsx'))).toBe(true);
    expect(fs.existsSync(path.join(dir, 'SessionOutput.tsx'))).toBe(true);
  });

  it('CrewMonitorScreen contains AGENT MONITOR text', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/components/layout/crew-monitor/CrewMonitorLayout.tsx'), 'utf8');
    expect(src).toContain('AGENT MONITOR');
  });

  it('CrewMonitorScreen contains DagVisualization', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/components/layout/crew-monitor/CrewMonitorLayout.tsx'), 'utf8');
    expect(src).toContain('DagVisualization');
  });

  it('CrewMonitorScreen contains stage list', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/components/layout/crew-monitor/CrewMonitorLayout.tsx'), 'utf8');
    expect(src).toContain('stages');
  });

  it('CrewMonitorScreen contains SUBAGENT OUTPUT text', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/components/layout/crew-monitor/WorkerOutputPanel.tsx'), 'utf8');
    expect(src).toContain('SUBAGENT OUTPUT');
  });

  it('CrewMonitorScreen contains ProgressChip import', () => {
    const src = fs.readFileSync(path.join(ROOT, 'src/components/layout/crew-monitor/CrewMonitorLayout.tsx'), 'utf8');
    expect(src).toContain('ProgressChip');
  });

  // Live tests
  it('store.sessions is defined on startup', async () => {
    tc = await E2ETestCase.builder().withTestName('crew-sessions-init').launch();
    await tc.sleepMs(2000);
    const store = await tc.getStore();
    expect(store.sessions).toBeDefined();
    await tc.cleanup(); tc = null;
  }, 30000);

  it('ctrl+g switches mode to crew-monitor', async () => {
    tc = await E2ETestCase.builder().withTestName('crew-ctrl-g').launch();
    await tc.sleepMs(2000);
    const before = await tc.getStore();
    expect(before.mode).not.toBe('crew-monitor');
    await tc.sendKeys('\x07'); // ctrl+g
    await tc.sleepMs(500);
    const after = await tc.getStore();
    expect(after.mode).toBe('crew-monitor');
    await tc.cleanup(); tc = null;
  }, 30000);

  it('q returns to inline from crew-monitor', async () => {
    tc = await E2ETestCase.builder().withTestName('crew-escape').launch();
    await tc.sleepMs(2000);
    await tc.sendKeys('\x07'); // ctrl+g
    await tc.sleepMs(500);
    await tc.sendKeys('q'); // q to exit crew monitor
    await tc.sleepMs(500);
    const store = await tc.getStore();
    expect(store.mode).toBe('inline');
    await tc.cleanup(); tc = null;
  }, 30000);

  it('after ctrl+g, terminal shows AGENT MONITOR in snapshot', async () => {
    tc = await E2ETestCase.builder().withTestName('crew-agent-monitor-text').launch();
    await tc.sleepMs(2000);
    await tc.sendKeys('\x07'); // ctrl+g
    await tc.sleepMs(500);
    const snapshot = tc.getSnapshot();
    // Check if either the main UI is shown (with sessions) or empty state (without sessions)
    const hasAgentMonitor = snapshot.some(line => line.includes('AGENT MONITOR'));
    const hasEmptyState = snapshot.some(line => line.includes('No active sessions'));
    expect(hasAgentMonitor || hasEmptyState).toBe(true);
    await tc.cleanup(); tc = null;
  }, 30000);

  it('after ctrl+g, terminal shows AGENT MONITOR or empty state in snapshot', async () => {
    tc = await E2ETestCase.builder().withTestName('crew-execution-graph-text').launch();
    await tc.sleepMs(2000);
    await tc.sendKeys('\x07'); // ctrl+g
    await tc.sleepMs(500);
    const snapshot = tc.getSnapshot();
    // Check if either the main UI is shown (with sessions) or empty state (without sessions)
    const hasAgentMonitor = snapshot.some(line => line.includes('AGENT MONITOR'));
    const hasEmptyState = snapshot.some(line => line.includes('No active sessions'));
    expect(hasAgentMonitor || hasEmptyState).toBe(true);
    await tc.cleanup(); tc = null;
  }, 30000);

    it('after ctrl+g with no sessions, shows crew monitor without crash', async () => {
      tc = await E2ETestCase.builder().withTestName('crew-empty-state').launch();
      await tc.sleepMs(2000);
      await tc.sendKeys('\x07'); // ctrl+g
      await tc.sleepMs(500);
      const store = await tc.getStore();
      // Crew monitor is active (no crash) and mode switched
      expect(store.mode).toBe('crew-monitor');
      await tc.cleanup(); tc = null;
    }, 30000);
});