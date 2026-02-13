/**
 * E2E test measuring input latency as conversation grows.
 * Tests with small, medium, and large responses.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { readFileSync } from 'fs';
import { E2ETestCase } from './E2ETestCase';

interface InputMetricSample {
  total: number;
}

function parseInputMetrics(logPath: string): InputMetricSample[] {
  const content = readFileSync(logPath, 'utf-8');
  const regex = /\[InputMetrics\].*total=([0-9.-]+)ms/g;
  const samples: InputMetricSample[] = [];
  let match;
  while ((match = regex.exec(content)) !== null) {
    samples.push({ total: parseFloat(match[1]) });
  }
  return samples;
}

function computeStats(samples: InputMetricSample[]) {
  if (samples.length === 0) return null;
  const totals = samples.map(s => s.total).sort((a, b) => a - b);
  const percentile = (arr: number[], p: number) => arr[Math.ceil((p / 100) * arr.length) - 1] ?? 0;
  return { p50: percentile(totals, 50), p95: percentile(totals, 95), max: totals[totals.length - 1] };
}

async function runLatencyTest(testCase: E2ETestCase, numTurns: number, responseText: string) {
  const charsPerTurn = 3;
  
  // Small delay to ensure IPC is fully ready
  await testCase.sleepMs(500);
  
  for (let turn = 0; turn < numTurns; turn++) {
    await testCase.pushSendMessageResponse([
      { kind: 'event', data: { kind: 'AssistantResponseEvent', data: { content: `R${turn + 1}: ${responseText}` } } },
    ], { silent: true });
    await testCase.pushSendMessageResponse(null, { silent: true });

    for (const char of `M${turn + 1}`) {
      await testCase.sendKeys(char);
      await testCase.sleepMs(30);
    }
    
    await testCase.pressEnter();
    await testCase.waitForText(`R${turn + 1}:`, 30000);
    await testCase.sleepMs(200);
  }

  await testCase.pressCtrlCTwice();
  await testCase.expectExit();

  const samples = parseInputMetrics(testCase.getTuiLogPath());
  const stats = computeStats(samples);
  const turn1 = computeStats(samples.slice(0, charsPerTurn));
  const turnN = computeStats(samples.slice(-charsPerTurn));
  
  return { samples: samples.length, stats, turn1, turnN };
}

describe('Input Latency', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('small responses (~20 chars)', async () => {
    testCase = await E2ETestCase.builder().withTestName('latency-small').withTimeout(120000).launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const result = await runLatencyTest(testCase, 50, 'Short response here.');
    
    console.log(`Small: P50=${result.stats?.p50.toFixed(1)}ms P95=${result.stats?.p95.toFixed(1)}ms | Turn1=${result.turn1?.p50.toFixed(1)}ms Turn50=${result.turnN?.p50.toFixed(1)}ms`);
    expect(result.samples).toBeGreaterThan(0);
    expect(result.stats!.p50).toBeLessThan(25);
    expect(result.stats!.p95).toBeLessThan(50);
  }, 180000);

  it('medium responses (~500 chars)', async () => {
    testCase = await E2ETestCase.builder().withTestName('latency-medium').withTimeout(120000).launch();
    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    const mediumText = 'This is a medium-length response with some code:\n```ts\nconst x = 1;\n```\nAnd more text here.'.repeat(5);
    const result = await runLatencyTest(testCase, 50, mediumText);
    
    console.log(`Medium: P50=${result.stats?.p50.toFixed(1)}ms P95=${result.stats?.p95.toFixed(1)}ms | Turn1=${result.turn1?.p50.toFixed(1)}ms Turn50=${result.turnN?.p50.toFixed(1)}ms`);
    expect(result.samples).toBeGreaterThan(0);
    expect(result.stats!.p50).toBeLessThan(35);
    expect(result.stats!.p95).toBeLessThan(75);
  }, 180000);
});
