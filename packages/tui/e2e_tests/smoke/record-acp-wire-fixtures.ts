#!/usr/bin/env bun

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';
import { parseArgs } from 'node:util';
import { filterScenarios, loadScenarios, type Scenario } from './scenario-runner';
import type {
  AcpPromptResponseFixture,
  AcpRecordedServerMessage,
  AcpWireScenarioFixture,
  AcpWireTurnFixture,
} from '../scenario-runner/acp-wire-fixtures';
import { computePromptContractHash } from '../scenario-runner/acp-wire-fixtures';

const REPO_TUI_ROOT = path.resolve(__dirname, '../..');
const DEFAULT_OUTPUT_DIR = path.join(__dirname, 'fixtures', 'acp-wire');
const STEP_SETTLE_DELAY_MS = 200;
const TYPE_CHAR_DELAY_MS = 30;
const DEFAULT_TIMEOUT_MS = 120_000;
const READY_TIMEOUT_MS = 60_000;
const REQUEST_POLL_MS = 250;

interface CliOptions {
  scenarios?: string[];
  categories?: string[];
  all: boolean;
  outputDir: string;
  timeoutMs: number;
}

interface KnightRiderSession {
  port: number;
  outputDir: string;
  recordPath: string;
  child: Bun.Subprocess<'ignore', 'pipe', 'pipe'>;
  stdoutBuffer: { text: string };
  stderrBuffer: { text: string };
}

interface TraceLine {
  ts: number;
  dir: 'in' | 'out';
  msg: Record<string, unknown>;
}

interface ParsedPromptTurn {
  promptText: string;
  serverMessages: AcpRecordedServerMessage[];
  promptResponse?: AcpPromptResponseFixture;
  durationMs?: number;
}

const FILTER_BACKEND = {
  id: 'live' as const,
  engine: 'kas' as const,
  async launch() {
    throw new Error('record-acp-wire backend is only used for scenario filtering');
  },
};

function parseCliArgs(): CliOptions {
  const { values } = parseArgs({
    options: {
      scenario: { type: 'string', short: 's', multiple: true, default: [] },
      category: { type: 'string', short: 'c', multiple: true, default: [] },
      all: { type: 'boolean', default: false },
      output: { type: 'string', short: 'o', default: DEFAULT_OUTPUT_DIR },
      timeout: { type: 'string', default: String(DEFAULT_TIMEOUT_MS) },
    },
    strict: true,
    allowPositionals: false,
  });

  return {
    scenarios:
      (values.scenario as string[]).length > 0
        ? ((values.scenario as string[]) ?? [])
        : undefined,
    categories:
      (values.category as string[]).length > 0
        ? ((values.category as string[]) ?? [])
        : undefined,
    all: values.all ?? false,
    outputDir: (values.output as string) ?? DEFAULT_OUTPUT_DIR,
    timeoutMs: parseInt((values.timeout as string) ?? String(DEFAULT_TIMEOUT_MS), 10),
  };
}

function needsFixture(scenario: Scenario): boolean {
  return scenario.steps.some((step) => step.startsWith('prompt:'));
}

function promptSteps(scenario: Scenario): Array<{ stepIndex: number; promptText: string }> {
  return scenario.steps
    .map((step, index) => ({ step, index }))
    .filter(({ step }) => step.startsWith('prompt:'))
    .map(({ step, index }) => ({
      stepIndex: index,
      promptText: step.substring('prompt:'.length),
    }));
}

async function readStream(
  stream: ReadableStream<Uint8Array> | null,
  buffer: { text: string }
): Promise<void> {
  if (!stream) return;
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer.text += decoder.decode(value, { stream: true });
    }
    buffer.text += decoder.decode();
  } finally {
    reader.releaseLock();
  }
}

function choosePort(): number {
  return 3001 + Math.floor(Math.random() * 5000);
}

async function startKnightRider(scenarioId: string): Promise<KnightRiderSession> {
  const port = choosePort();
  const outputDir = fs.mkdtempSync(
    path.join(os.tmpdir(), `kiro-kr-${scenarioId}-`)
  );
  const recordPath = path.join(outputDir, 'acp-trace.jsonl');

  const stdoutBuffer = { text: '' };
  const stderrBuffer = { text: '' };
  const child = Bun.spawn({
    cmd: [
      process.execPath,
      'e2e_tests/knight-rider.ts',
      '--kas',
      '--port',
      String(port),
      '--out',
      outputDir,
    ],
    cwd: REPO_TUI_ROOT,
    env: {
      ...process.env,
      KIRO_ACP_RECORD_PATH: recordPath,
    },
    stdout: 'pipe',
    stderr: 'pipe',
  });

  void readStream(child.stdout, stdoutBuffer);
  void readStream(child.stderr, stderrBuffer);

  const session: KnightRiderSession = {
    port,
    outputDir,
    recordPath,
    child,
    stdoutBuffer,
    stderrBuffer,
  };

  await waitForKnightRiderReady(session, READY_TIMEOUT_MS);
  return session;
}

async function stopKnightRider(session: KnightRiderSession): Promise<void> {
  session.child.kill('SIGINT');
  await session.child.exited;
}

async function waitForKnightRiderReady(
  session: KnightRiderSession,
  timeoutMs: number
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (session.child.exitCode !== null) {
      throw new Error(
        `Knight Rider exited before becoming ready.\nstdout:\n${session.stdoutBuffer.text}\nstderr:\n${session.stderrBuffer.text}`
      );
    }
    try {
      const response = await fetch(
        `http://127.0.0.1:${session.port}/api/status`
      );
      if (response.ok) {
        const body = (await response.json()) as {
          ready?: boolean;
          error?: string | null;
        };
        if (body.ready) return;
        if (body.error) {
          throw new Error(body.error);
        }
      }
    } catch {
      // Keep polling until timeout.
    }
    await sleep(REQUEST_POLL_MS);
  }
  throw new Error(
    `Timed out waiting for Knight Rider to become ready.\nstdout:\n${session.stdoutBuffer.text}\nstderr:\n${session.stderrBuffer.text}`
  );
}

async function postJson<T = unknown>(
  port: number,
  route: string,
  body: Record<string, unknown> = {}
): Promise<T> {
  const response = await fetch(`http://127.0.0.1:${port}${route}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`${route} failed (${response.status}): ${text}`);
  }
  return (await response.json()) as T;
}

async function typeText(port: number, text: string): Promise<void> {
  for (const char of text) {
    await postJson(port, '/api/keys', { keys: char });
    await sleep(TYPE_CHAR_DELAY_MS);
  }
}

function parseTraceLines(recordPath: string): TraceLine[] {
  if (!fs.existsSync(recordPath)) return [];
  const raw = fs.readFileSync(recordPath, 'utf-8');
  return raw
    .split('\n')
    .map((line) => line.trim())
    .filter(Boolean)
    .flatMap((line) => {
      try {
        return [JSON.parse(line) as TraceLine];
      } catch {
        return [];
      }
    });
}

function summarizePromptTrace(recordPath: string): {
  requests: number;
  responses: number;
} {
  const lines = parseTraceLines(recordPath);
  let requests = 0;
  let responses = 0;
  const pendingIds = new Set<string>();

  for (const line of lines) {
    const message = line.msg;
    if (line.dir === 'out' && message.method === 'session/prompt') {
      requests++;
      if (message.id !== undefined) {
        pendingIds.add(String(message.id));
      }
      continue;
    }
    if (
      line.dir === 'in' &&
      message.method === undefined &&
      message.id !== undefined &&
      pendingIds.has(String(message.id))
    ) {
      responses++;
      pendingIds.delete(String(message.id));
    }
  }

  return { requests, responses };
}

async function waitForPromptResponses(
  recordPath: string,
  expectedResponses: number,
  timeoutMs: number
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (summarizePromptTrace(recordPath).responses >= expectedResponses) {
      return;
    }
    await sleep(REQUEST_POLL_MS);
  }
  throw new Error(
    `Timed out waiting for prompt response ${expectedResponses} in ${recordPath}`
  );
}

async function executeScenarioSteps(
  session: KnightRiderSession,
  scenario: Scenario,
  timeoutMs: number
): Promise<void> {
  let issuedPromptCount = 0;

  for (const step of scenario.steps) {
    const colonIndex = step.indexOf(':');
    const command = colonIndex === -1 ? step : step.substring(0, colonIndex);
    const arg = colonIndex === -1 ? '' : step.substring(colonIndex + 1);

    switch (command) {
      case 'type':
        await typeText(session.port, arg);
        break;
      case 'enter':
        await postJson(session.port, '/api/enter');
        break;
      case 'waitForText':
        await postJson(session.port, '/api/wait-for-text', {
          text: arg,
          timeout: timeoutMs,
        });
        break;
      case 'waitForIdle':
        await waitForPromptResponses(
          session.recordPath,
          issuedPromptCount,
          timeoutMs
        );
        break;
      case 'prompt':
        await typeText(session.port, arg);
        await postJson(session.port, '/api/enter');
        issuedPromptCount++;
        break;
      case 'ctrlc':
        await postJson(session.port, '/api/ctrlc');
        break;
      case 'ctrlc-twice':
        await postJson(session.port, '/api/ctrlc');
        await postJson(session.port, '/api/ctrlc');
        break;
      case 'ctrlj':
        await postJson(session.port, '/api/keys', { keys: '\n' });
        break;
      case 'ctrls':
        await postJson(session.port, '/api/keys', {
          keys: String.fromCharCode(0x13),
        });
        break;
      case 'arrowUp':
        await postJson(session.port, '/api/up');
        break;
      case 'arrowDown':
        await postJson(session.port, '/api/down');
        break;
      case 'escape':
        await postJson(session.port, '/api/escape');
        break;
      case 'tab':
        await postJson(session.port, '/api/keys', { keys: '\t' });
        break;
      case 'sleep':
        await sleep(parseInt(arg, 10));
        break;
      case 'frame':
        await postJson(session.port, '/api/frame', { label: arg });
        break;
      default:
        throw new Error(`Unsupported scenario step for Knight Rider recorder: ${step}`);
    }

    await sleep(STEP_SETTLE_DELAY_MS);
  }
}

function extractPromptText(params: unknown): string {
  const blocks =
    (params as { prompt?: Array<{ type?: string; text?: string }> } | undefined)
      ?.prompt ?? [];
  return blocks
    .filter((block) => block.type === 'text')
    .map((block) => block.text ?? '')
    .join('');
}

function parseTraceTurns(recordPath: string): ParsedPromptTurn[] {
  const lines = parseTraceLines(recordPath);
  const turns: ParsedPromptTurn[] = [];
  let active:
    | (ParsedPromptTurn & {
        requestId: string;
        startedAt: number;
      })
    | null = null;

  for (const line of lines) {
    const message = line.msg;
    if (line.dir === 'out' && message.method === 'session/prompt') {
      if (active) {
        turns.push({
          promptText: active.promptText,
          serverMessages: active.serverMessages,
          promptResponse: active.promptResponse,
          durationMs: active.durationMs,
        });
      }
      active = {
        requestId: String(message.id ?? ''),
        startedAt: line.ts,
        promptText: extractPromptText(message.params),
        serverMessages: [],
      };
      continue;
    }

    if (!active) continue;

    if (
      line.dir === 'in' &&
      message.method === undefined &&
      String(message.id ?? '') === active.requestId
    ) {
      active.promptResponse = {};
      if ('result' in message) {
        active.promptResponse.result = message.result;
      }
      if ('error' in message && message.error) {
        active.promptResponse.error = message.error as AcpPromptResponseFixture['error'];
      }
      active.durationMs = line.ts - active.startedAt;
      turns.push({
        promptText: active.promptText,
        serverMessages: active.serverMessages,
        promptResponse: active.promptResponse,
        durationMs: active.durationMs,
      });
      active = null;
      continue;
    }

    if (line.dir !== 'in' || message.method === undefined) {
      continue;
    }

    active.serverMessages.push({
      kind: message.id === undefined ? 'notification' : 'request',
      method: String(message.method),
      params: message.params,
    });
  }

  if (active) {
    turns.push({
      promptText: active.promptText,
      serverMessages: active.serverMessages,
      promptResponse: active.promptResponse,
      durationMs: active.durationMs,
    });
  }

  return turns;
}

function buildFixture(
  scenario: Scenario,
  parsedTurns: ParsedPromptTurn[]
): AcpWireScenarioFixture {
  const prompts = promptSteps(scenario);
  if (parsedTurns.length !== prompts.length) {
    throw new Error(
      `Prompt turn count mismatch for ${scenario.id}: expected ${prompts.length}, got ${parsedTurns.length}`
    );
  }

  const turns: AcpWireTurnFixture[] = prompts.map((prompt, index) => ({
    stepIndex: prompt.stepIndex,
    promptText: prompt.promptText,
    serverMessages: parsedTurns[index]?.serverMessages ?? [],
    promptResponse: parsedTurns[index]?.promptResponse,
    durationMs: parsedTurns[index]?.durationMs,
  }));

  return {
    schemaVersion: 2,
    scenarioId: scenario.id,
    backend: 'acp-mock',
    engine: 'kas',
    contractHash: computePromptContractHash(scenario),
    turns,
  };
}

function writeFixtureAtomic(filePath: string, fixture: AcpWireScenarioFixture): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  const tmpPath = `${filePath}.tmp.${process.pid}`;
  fs.writeFileSync(tmpPath, JSON.stringify(fixture, null, 2));
  fs.renameSync(tmpPath, filePath);
}

async function recordScenario(
  scenario: Scenario,
  outputDir: string,
  timeoutMs: number
): Promise<void> {
  const session = await startKnightRider(scenario.id);
  try {
    await executeScenarioSteps(session, scenario, timeoutMs);
    await sleep(500);
  } finally {
    await stopKnightRider(session);
  }

  const parsedTurns = parseTraceTurns(session.recordPath);
  const fixture = buildFixture(scenario, parsedTurns);
  writeFixtureAtomic(path.join(outputDir, `${scenario.id}.json`), fixture);
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function main(): Promise<void> {
  const cli = parseCliArgs();
  const allScenarios = loadScenarios();
  const filtered = cli.all
    ? allScenarios
    : filterScenarios(allScenarios, {
        backend: FILTER_BACKEND,
        scenarios: cli.scenarios,
        categories: cli.categories,
      });

  const scenarios = filtered.filter(needsFixture);
  if (scenarios.length === 0) {
    throw new Error('No prompt scenarios matched the requested filters');
  }

  console.log(
    `[recorder] recording ${scenarios.length} prompt scenario(s) to ${cli.outputDir}`
  );

  for (const [index, scenario] of scenarios.entries()) {
    console.log(`[recorder] ${index + 1}/${scenarios.length}: ${scenario.id}`);
    await recordScenario(scenario, cli.outputDir, cli.timeoutMs);
  }

  console.log('[recorder] complete');
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[recorder] ${message}`);
  process.exit(1);
});
