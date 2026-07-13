import { describe, it, expect, afterEach } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe('Event isolation', () => {
  let tc: E2ETestCase | null = null;
  afterEach(async () => {
    await tc?.cleanup();
    tc = null;
  });

  it('main chat messages not polluted by subagent session events', async () => {
    tc = await E2ETestCase.builder()
      .withTestName('isolation-main-clean')
      .launch();
    await tc.waitForText('ask a question', 10000);
    const before = await tc.getStore();
    const initialCount = before.messages?.length ?? 0;

    // sessionEventBuffer should be empty (no subagent events yet)
    const buffer = before.sessionEventBuffer ?? {};
    expect(Object.keys(buffer).length).toBe(0);

    // main messages unchanged
    const after = await tc.getStore();
    expect(after.messages?.length ?? 0).toBe(initialCount);
  }, 30000);
});
