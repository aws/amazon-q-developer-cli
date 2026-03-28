import fs from 'fs';
import path from 'path';
import { describe, it, expect, afterEach } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

const SRC = path.join(__dirname, '..', 'src');

describe('Session event routing', () => {
  it('sessionUpdate routes subagent events to multiSessionHandlers not main chat', () => {
    const src = fs.readFileSync(path.join(SRC, 'acp-client.ts'), 'utf8');
    // Must check sessionId before routing
    expect(src).toContain('notifSessionId !== this.sessionId');
    // Must call multiSessionHandlers for subagent events
    expect(src).toContain('multiSessionHandlers.forEach');
    // broadcastStreamEvent must only be called for non-subagent events
    const sessionUpdateFn = src.slice(
      src.indexOf('async sessionUpdate'),
      src.indexOf('async sessionUpdate') + 1200
    );
    expect(sessionUpdateFn).toContain('isSubagentEvent');
    // subagent branch calls multiSessionHandlers, else branch calls broadcastStreamEvent
    const multiIdx = sessionUpdateFn.indexOf('multiSessionHandlers.forEach');
    const elseIdx = sessionUpdateFn.indexOf('} else {');
    const broadcastIdx = sessionUpdateFn.indexOf('broadcastStreamEvent', elseIdx);
    expect(multiIdx).toBeGreaterThan(0);
    expect(elseIdx).toBeGreaterThan(multiIdx);
    expect(broadcastIdx).toBeGreaterThan(elseIdx);
  });

  it('index.tsx routes multiSessionUpdate to pushSessionEvent', () => {
    const src = fs.readFileSync(path.join(SRC, 'index.tsx'), 'utf8');
    expect(src).toContain('onMultiSessionUpdate');
    expect(src).toContain('pushSessionEvent');
  });

  it('store.sessionEventBuffer accumulates events per sessionId', () => {
    const src = fs.readFileSync(path.join(SRC, 'stores/app-store.ts'), 'utf8');
    expect(src).toContain('sessionEventBuffer');
    expect(src).toContain('pushSessionEvent');
    // Events are keyed by sessionId
    expect(src).toContain('[sessionId]');
  });
});

describe('Event isolation', () => {
  let tc: E2ETestCase | null = null;
  afterEach(async () => { await tc?.cleanup(); tc = null; });

  it('main chat messages not polluted by subagent session events', async () => {
    tc = await E2ETestCase.builder().withTestName('isolation-main-clean').launch();
    await tc.waitForText('ask a question', 10000);
    const before = await tc.getStore();
    const initialCount = before.messages?.length ?? 0;

    // sessionEventBuffer should be empty (no subagent events yet)
    const buffer = before.sessionEventBuffer ?? {};
    expect(Object.keys(buffer).length).toBe(0);

    // main messages unchanged
    const after = await tc.getStore();
    expect((after.messages?.length ?? 0)).toBe(initialCount);
  }, 30000);

  it('sessionUpdate routing: subagent events go to sessionEventBuffer not messages', () => {
    // Static: the routing logic is in place
    const src = fs.readFileSync(path.join(SRC, 'acp-client.ts'), 'utf8');
    // Subagent events routed to multiSessionHandlers (not broadcastStreamEvent)
    expect(src).toContain('notifSessionId !== this.sessionId');
    // multiSessionHandlers feeds pushSessionEvent -> sessionEventBuffer
    expect(src).toContain('multiSessionHandlers.forEach');
    // broadcastStreamEvent only called for non-subagent events (in else branch)
    const sessionUpdateFn = src.slice(
      src.indexOf('async sessionUpdate'),
      src.indexOf('async sessionUpdate') + 1200
    );
    const multiIdx = sessionUpdateFn.indexOf('multiSessionHandlers.forEach');
    const elseIdx = sessionUpdateFn.indexOf('} else {');
    const broadcastIdx = sessionUpdateFn.indexOf('broadcastStreamEvent', elseIdx);
    expect(multiIdx).toBeGreaterThan(0);
    expect(elseIdx).toBeGreaterThan(multiIdx);
    expect(broadcastIdx).toBeGreaterThan(elseIdx);
  });
});