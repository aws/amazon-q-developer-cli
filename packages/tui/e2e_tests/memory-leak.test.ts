/**
 * E2E memory regression tests for Ink rendering pipeline.
 *
 * Uses in-process memory reporting (process.memoryUsage() via IPC) and
 * forced GC to measure heap growth across multi-turn conversations.
 *
 * Key invariants tested:
 *  - Heap after GC stays within a bounded multiplier of baseline
 *  - Peak RSS for a 1MB single-chunk render stays under a hard cap
 *  - Long sessions (100 turns) don't show unbounded heap growth
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';
import type { MockStreamItem } from './types/chat-cli';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function mb(bytes: number): number {
  return Math.round(bytes / 1024 / 1024);
}

function generatePayload(sizeKb: number): string {
  const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 \n';
  let result = '';
  const target = sizeKb * 1024;
  while (result.length < target) {
    result += chars[Math.floor(Math.random() * chars.length)];
  }
  return result;
}

function assistantResponse(content: string): MockStreamItem[] {
  return [{ kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content } } }];
}

/** Measure baseline heap after GC. */
async function measureBaseline(tc: E2ETestCase) {
  await tc.forceGC();
  await tc.sleepMs(500);
  return tc.getMemoryUsage();
}

/** Run one conversation turn and return memory after it settles. */
async function runTurn(tc: E2ETestCase, turnIndex: number, events: MockStreamItem[]) {
  await tc.pushSendMessageResponse(events);
  await tc.pushSendMessageResponse(null);
  tc.sendKeys(`q${turnIndex}`);
  await tc.sleepMs(100);
  tc.pressEnter();
  await tc.waitForIdle();
  return tc.getMemoryUsage();
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('Memory Regression', () => {
  let tc: E2ETestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.pressCtrlCTwice();
      await tc.cleanup();
      tc = null;
    }
  });

  async function setup(name: string, timeout = 300_000) {
    tc = await E2ETestCase.builder().withTestName(name).withTimeout(timeout).launch();
    await tc.waitForText('ask a question', 10_000);
    await tc.getSessionId();
    return tc;
  }

  // ---- Single large chunk: the original amplification scenario ----------

  it('single 1MB response stays under 1.5GB peak RSS', async () => {
    const t = await setup('mem-single-1mb');
    const baseline = await measureBaseline(t);

    const mem = await runTurn(t, 0, assistantResponse(generatePayload(1024)));

    console.log(`1MB chunk: baseline RSS=${mb(baseline.rss)}MB, peak RSS=${mb(mem.rss)}MB, heap=${mb(mem.heapUsed)}MB`);

    // Stock ink 6.6.0 hit 5.5GB here. With our patches we expect <1.5GB.
    expect(mb(mem.rss)).toBeLessThan(1500);
  }, 300_000);

  // ---- Multi-turn: heap should be collectible --------------------------

  it('10x50KB turns — heap after GC within 3x baseline', async () => {
    const t = await setup('mem-10x50kb');
    const baseline = await measureBaseline(t);

    for (let i = 0; i < 10; i++) {
      await runTurn(t, i, assistantResponse(generatePayload(50)));
    }

    await t.forceGC();
    await t.sleepMs(1000);
    const after = await t.getMemoryUsage();

    console.log(`10x50KB: baseline heap=${mb(baseline.heapUsed)}MB, after-gc heap=${mb(after.heapUsed)}MB`);
    expect(mb(after.heapUsed)).toBeLessThan(mb(baseline.heapUsed) * 3);
  }, 300_000);

  it('5x200KB turns — heap after GC within 3x baseline', async () => {
    const t = await setup('mem-5x200kb');
    const baseline = await measureBaseline(t);

    for (let i = 0; i < 5; i++) {
      await runTurn(t, i, assistantResponse(generatePayload(200)));
    }

    await t.forceGC();
    await t.sleepMs(1000);
    const after = await t.getMemoryUsage();

    console.log(`5x200KB: baseline heap=${mb(baseline.heapUsed)}MB, after-gc heap=${mb(after.heapUsed)}MB`);
    expect(mb(after.heapUsed)).toBeLessThan(mb(baseline.heapUsed) * 3);
  }, 300_000);

  // ---- Long session: no unbounded growth --------------------------------

  it('50x50KB long session — heap does not grow unbounded', async () => {
    const t = await setup('mem-50x50kb', 600_000);
    const baseline = await measureBaseline(t);

    let peakRss = 0;
    for (let i = 0; i < 50; i++) {
      const mem = await runTurn(t, i, assistantResponse(generatePayload(50)));
      if (mem.rss > peakRss) peakRss = mem.rss;
    }

    await t.forceGC();
    await t.sleepMs(1000);
    const after = await t.getMemoryUsage();

    console.log(`50x50KB: baseline heap=${mb(baseline.heapUsed)}MB, peak RSS=${mb(peakRss)}MB, after-gc heap=${mb(after.heapUsed)}MB`);

    // Heap should be collectible — within 4x baseline even after 50 turns
    expect(mb(after.heapUsed)).toBeLessThan(mb(baseline.heapUsed) * 4);
    // Peak RSS should stay under 700MB
    expect(mb(peakRss)).toBeLessThan(700);
  }, 600_000);

  // ---- Tool use turns: tool events + assistant response -----------------

  it('5 turns with tool calls — heap stays bounded', async () => {
    const t = await setup('mem-tool-calls');
    const baseline = await measureBaseline(t);

    for (let i = 0; i < 5; i++) {
      const events: MockStreamItem[] = [];
      for (let j = 0; j < 3; j++) {
        events.push({
          kind: 'event',
          data: { kind: 'ToolUseEvent', data: { tool_use_id: `tool-${i}-${j}`, name: 'fs_read', input: JSON.stringify({ ops: [{ path: `file-${j}.ts` }] }), stop: true } },
        });
      }
      events.push(...assistantResponse(`Done turn ${i}: ${generatePayload(50)}`));
      await tc!.pushSendMessageResponse(events);
      await tc!.pushSendMessageResponse(null);
      tc!.sendKeys(`q${i}`);
      await tc!.sleepMs(100);
      tc!.pressEnter();
      // Tool calls stay in_progress (no tool results in mock), but text still renders
      await tc!.waitForText(`Done turn ${i}`, 30_000);
      await tc!.sleepMs(500);
    }

    await t.forceGC();
    await t.sleepMs(1000);
    const after = await t.getMemoryUsage();

    console.log(`5x3-tool: baseline heap=${mb(baseline.heapUsed)}MB, after-gc heap=${mb(after.heapUsed)}MB`);
    expect(mb(after.heapUsed)).toBeLessThan(mb(baseline.heapUsed) * 3);
  }, 300_000);

  // ---- Varying payload sizes: realistic conversation --------------------

  it('20 turns with varying payloads (10-150KB) — heap stays bounded', async () => {
    const t = await setup('mem-varying');
    const baseline = await measureBaseline(t);
    const sizes = [10, 50, 100, 25, 75, 150];

    for (let i = 0; i < 20; i++) {
      const sizeKb = sizes[i % sizes.length];
      await runTurn(t, i, assistantResponse(generatePayload(sizeKb)));
    }

    await t.forceGC();
    await t.sleepMs(1000);
    const after = await t.getMemoryUsage();

    console.log(`20x varying: baseline heap=${mb(baseline.heapUsed)}MB, after-gc heap=${mb(after.heapUsed)}MB`);
    expect(mb(after.heapUsed)).toBeLessThan(mb(baseline.heapUsed) * 3);
  }, 300_000);
});
