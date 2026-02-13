/**
 * Quick 5-message test to validate input metrics are logging
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { readFileSync } from 'fs';
import { E2ETestCase } from './E2ETestCase';

describe('Input Latency Quick', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('5 messages - validate metrics logging', async () => {
    testCase = await E2ETestCase.builder().withTestName('latency-quick').withTimeout(60000).launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    for (let turn = 0; turn < 5; turn++) {
      await testCase.pushSendMessageResponse([
        { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: `R${turn + 1}: Short` } } },
      ], { silent: true });
      await testCase.pushSendMessageResponse(null, { silent: true });

      await testCase.sendKeys(`M${turn + 1}`);
      await testCase.sleepMs(50);
      await testCase.pressEnter();
      await testCase.waitForText(`R${turn + 1}:`, 10000);
      await testCase.sleepMs(100);
    }

    await testCase.pressCtrlCTwice();
    await testCase.expectExit();

    const logPath = testCase.getTuiLogPath();
    const content = readFileSync(logPath, 'utf-8');
    
    const metricsEnabled = content.includes('[InputMetrics] Input latency metrics enabled');
    const regex = /\[InputMetrics\].*total=([0-9.-]+)ms/g;
    const samples: number[] = [];
    let match;
    while ((match = regex.exec(content)) !== null) {
      samples.push(parseFloat(match[1]!));
    }
    
    const sorted = samples.sort((a, b) => a - b);
    const p50 = sorted[Math.ceil(0.5 * sorted.length) - 1] ?? 0;
    const p95 = sorted[Math.ceil(0.95 * sorted.length) - 1] ?? 0;
    
    console.log(`Metrics enabled: ${metricsEnabled}`);
    console.log(`Metric samples found: ${samples.length}`);
    console.log(`P50=${p50.toFixed(1)}ms P95=${p95.toFixed(1)}ms`);
    console.log(`Log path: ${logPath}`);
    
    expect(metricsEnabled).toBe(true);
    expect(samples.length).toBeGreaterThan(0);
  }, 90000);
});
