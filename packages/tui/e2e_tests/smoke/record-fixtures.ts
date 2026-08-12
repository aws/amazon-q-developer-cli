#!/usr/bin/env bun
/**
 * record-fixtures.ts — Fixture Recorder for Testing Certification
 *
 * Records ACP response events produced by the real agent when running scenarios
 * that contain `prompt` steps. These fixtures enable deterministic replay
 * without an LLM.
 *
 * Usage:
 *   bun record-fixtures.ts [options]
 *
 * Options:
 *   --scenario <id>     Record a single scenario
 *   --category <name>   Filter by category
 *   --all               Record all eligible scenarios
 *   --check             Report stale/missing fixtures (exit 4 if any)
 *   --force             Re-record even if contractHash matches
 *   --output <dir>      Output directory (default: ./fixtures/acp-wire)
 *   --engine <v2|kas>   Engine to record against (default: v2)
 *   --timeout <ms>      Per-scenario timeout (default: 120000)
 *   --dry-run           Show what would be recorded without running
 *
 * Exit codes:
 *   0 — Success
 *   1 — Recording failure(s)
 *   2 — Invalid arguments
 *   4 — Stale/missing fixtures (--check mode)
 */

import * as fs from 'node:fs';
import * as path from 'node:path';
import * as crypto from 'node:crypto';
import { parseArgs } from 'node:util';
import { spawn } from 'node:child_process';
import { Readable, Writable } from 'node:stream';
import * as acp from '@agentclientprotocol/sdk';
import { loadScenarios, filterScenarios, type Scenario, type Engine } from './scenario-runner';
import { requireChatCliBin } from '../../src/test-utils/chat-cli-bin';
import type { MockStreamItem } from '../types/chat-cli';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface PromptFixture {
  stepIndex: number;
  promptText: string;
  events: MockStreamItem[];
  durationMs: number;
}

interface ScenarioFixture {
  schemaVersion: 1;
  scenarioId: string;
  recordedAt: string;
  engine: Engine;
  engineVersion: string;
  contractHash: string;
  prompts: PromptFixture[];
}

interface RecordResult {
  scenarioId: string;
  status: 'recorded' | 'skipped' | 'up-to-date' | 'failed' | 'stale' | 'missing';
  error?: string;
  fixture?: ScenarioFixture;
}

const FILTER_BACKEND = {
  id: 'live' as const,
  engine: 'v2' as Engine,
  async launch() {
    throw new Error('record-fixtures filter backend is never launched');
  },
};

// ---------------------------------------------------------------------------
// Contract hash — SHA-256 of the scenario's steps + verify contract
// ---------------------------------------------------------------------------

function computeContractHash(scenario: { steps: string[] }): string {
  const promptTexts = scenario.steps
    .filter((s: string) => s.startsWith('prompt:'))
    .map((s: string) => s.substring('prompt:'.length));
  const contract = JSON.stringify({ prompts: promptTexts });
  return crypto.createHash('sha256').update(contract).digest('hex');
}

// ---------------------------------------------------------------------------
// Fixture I/O
// ---------------------------------------------------------------------------

function fixturePath(outputDir: string, scenario: Scenario): string {
  return path.join(outputDir, `${scenario.id}.json`);
}

function loadFixture(filePath: string): ScenarioFixture | null {
  if (!fs.existsSync(filePath)) return null;
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf-8'));
  } catch {
    return null;
  }
}

function writeFixtureAtomic(filePath: string, fixture: ScenarioFixture): void {
  const dir = path.dirname(filePath);
  fs.mkdirSync(dir, { recursive: true });
  const tmp = `${filePath}.tmp.${process.pid}`;
  fs.writeFileSync(tmp, JSON.stringify(fixture, null, 2));
  fs.renameSync(tmp, filePath);
}

// ---------------------------------------------------------------------------
// Scenario eligibility — only scenarios with `prompt:` steps need fixtures
// ---------------------------------------------------------------------------

function needsFixture(scenario: Scenario): boolean {
  return scenario.steps.some((step) => step.startsWith('prompt:'));
}

function getPromptSteps(scenario: Scenario): Array<{ index: number; text: string }> {
  return scenario.steps
    .map((step, index) => ({ step, index }))
    .filter(({ step }) => step.startsWith('prompt:'))
    .map(({ step, index }) => ({ index, text: step.substring('prompt:'.length) }));
}

// ---------------------------------------------------------------------------
// Engine version (git short sha)
// ---------------------------------------------------------------------------

function getEngineVersion(): string {
  try {
    const result = Bun.spawnSync({
      cmd: ['git', 'rev-parse', '--short', 'HEAD'],
      cwd: path.resolve(__dirname, '../../../..'),
    });
    const sha = result.stdout.toString().trim();
    if (sha) return sha;
  } catch { /* fall through */ }
  return 'unknown';
}

// ---------------------------------------------------------------------------
// ACP recording session
//
// Approach:
// 1. Spawn `chat_cli acp` without KIRO_TEST_MODE (real LLM).
// 2. Create a session via ACP initialize + newSession.
// 3. For each prompt step, call connection.prompt() and collect all
//    session/update notifications that arrive during the call.
// 4. Map the collected ACP updates to MockStreamItem[] format.
//
// The `prompt()` method is a blocking RPC — it resolves only when the turn
// completes. Session/update notifications arrive as side effects during that
// window, which we buffer in `turnUpdates`.
// ---------------------------------------------------------------------------

type SessionUpdate = acp.SessionNotification['update'];

async function recordScenario(
  scenario: Scenario,
  opts: { engine: Engine; timeout: number }
): Promise<PromptFixture[]> {
  const chatPath = requireChatCliBin();
  const prompts = getPromptSteps(scenario);
  const fixtures: PromptFixture[] = [];

  let turnUpdates: SessionUpdate[] = [];

  const proc = spawn(chatPath, ['acp'], {
    stdio: ['pipe', 'pipe', 'pipe'],
    env: {
      ...process.env,
      KIRO_DISABLE_TELEMETRY: '1',
    },
  });

  if (!proc.stdin || !proc.stdout) {
    throw new Error('Failed to create ACP process stdio');
  }

  const output = Writable.toWeb(proc.stdin) as WritableStream<Uint8Array>;
  const input = Readable.toWeb(proc.stdout) as ReadableStream<Uint8Array>;
  const stream = acp.ndJsonStream(output, input);

  const connection = new acp.ClientSideConnection(
    () => ({
      async requestPermission(params: acp.RequestPermissionRequest) {
        const opt = (params.options as any[])?.find((o: any) => o.kind === 'allow_once')
          ?? (params.options as any[])?.[0];
        return { outcome: { outcome: 'selected' as const, optionId: opt?.optionId ?? '' } };
      },
      async sessionUpdate(notification: acp.SessionNotification) {
        if (notification.update) {
          turnUpdates.push(notification.update);
        }
      },
    }),
    stream,
  );

  try {
    await connection.initialize({
      protocolVersion: acp.PROTOCOL_VERSION,
      clientCapabilities: {},
    });

    const session = await (connection as any).newSession({
      cwd: process.cwd(),
      mcpServers: [],
    });

    const sessionId: string = session.sessionId;

    for (const prompt of prompts) {
      turnUpdates = [];
      const startMs = Date.now();

      const promptWithTimeout = Promise.race([
        (connection as any).prompt({
          sessionId,
          prompt: [{ type: 'text', text: prompt.text }],
        }),
        new Promise<never>((_, reject) =>
          setTimeout(
            () => reject(new Error(`Timeout recording prompt: "${prompt.text}"`)),
            opts.timeout,
          ),
        ),
      ]);

      await promptWithTimeout;
      const durationMs = Date.now() - startMs;

      const events = convertUpdatesToMockEvents(turnUpdates);

      fixtures.push({
        stepIndex: prompt.index,
        promptText: prompt.text,
        events,
        durationMs,
      });

      log('record', `  prompt[${fixtures.length - 1}]: "${prompt.text}" — ${events.length} events (${durationMs}ms)`);
    }

    return fixtures;
  } finally {
    proc.kill('SIGTERM');
    await new Promise((resolve) => setTimeout(resolve, 500));
    if (proc.exitCode === null) {
      proc.kill('SIGKILL');
    }
  }
}

// ---------------------------------------------------------------------------
// ACP update → MockStreamItem conversion
//
// Maps ACP session/update notifications to the ChatResponseStream-based
// MockStreamItem format used by E2ETestCase.pushSendMessageResponse().
// ---------------------------------------------------------------------------

function convertUpdatesToMockEvents(updates: SessionUpdate[]): MockStreamItem[] {
  const events: MockStreamItem[] = [];

  for (const update of updates) {
    switch (update.sessionUpdate) {
      case 'agent_message_chunk': {
        if (update.content.type === 'text') {
          events.push({
            kind: 'event',
            data: { kind: 'AssistantResponseEvent', data: { content: update.content.text } },
          });
        }
        break;
      }

      case 'tool_call': {
        const rawInput = update.rawInput;
        // Use update.kind (the tool type: "shell", "read", "write") as the tool name.
        // update.title is a display string (e.g. "Running: echo hello world") which
        // the agent cannot parse as a tool spec name.
        const toolName = (update as any).kind ?? update.title;
        events.push({
          kind: 'event',
          data: {
            kind: 'ToolUseEvent',
            data: {
              tool_use_id: update.toolCallId,
              name: toolName,
              input: typeof rawInput === 'string' ? rawInput : JSON.stringify(rawInput ?? ''),
              stop: true,
            },
          },
        });
        break;
      }

      case 'agent_thought_chunk': {
        if (update.content.type === 'text') {
          events.push({
            kind: 'event',
            data: {
              kind: 'ReasoningEvent',
              data: { text: update.content.text },
            },
          });
        }
        break;
      }

      case 'usage_update': {
        const contextUsage = (update as any).contextUsage;
        if (typeof contextUsage === 'number') {
          events.push({
            kind: 'event',
            data: { kind: 'ContextUsageEvent', data: { context_usage_percentage: contextUsage } },
          });
        }
        break;
      }

      // tool_call_update, session_info_update, available_commands_update, etc.
      // are internal to the agent lifecycle and not part of the mock response
      // stream. The fixture only records the agent's output content.
      default:
        break;
    }
  }

  // Terminate with MetadataEvent so replay knows the stream is done
  if (events.length > 0) {
    events.push({
      kind: 'event',
      data: { kind: 'MetadataEvent', data: { stop_reason: 'end_turn' } },
    });
  }

  return events;
}

// ---------------------------------------------------------------------------
// Logging
// ---------------------------------------------------------------------------

function log(prefix: string, msg: string): void {
  console.log(`[${prefix}] ${msg}`);
}

function logError(prefix: string, msg: string): void {
  console.error(`[${prefix}] ${msg}`);
}

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

interface CliOptions {
  scenario?: string;
  category?: string;
  all: boolean;
  check: boolean;
  force: boolean;
  output: string;
  engine: Engine;
  timeout: number;
  dryRun: boolean;
}

function parseCliArgs(): CliOptions {
  const { values } = parseArgs({
    options: {
      scenario: { type: 'string' },
      category: { type: 'string' },
      all: { type: 'boolean', default: false },
      check: { type: 'boolean', default: false },
      force: { type: 'boolean', default: false },
      output: { type: 'string', default: path.join(__dirname, 'fixtures', 'acp-wire') },
      engine: { type: 'string', default: 'v2' },
      timeout: { type: 'string', default: '120000' },
      'dry-run': { type: 'boolean', default: false },
      help: { type: 'boolean', default: false },
    },
    strict: true,
  });

  if (values.help) {
    console.log(`
Usage: bun record-fixtures.ts [options]

Options:
  --scenario <id>     Record a single scenario
  --category <name>   Filter by category
  --all               Record all eligible scenarios
  --check             Report stale/missing fixtures (exit 4 if any)
  --force             Re-record even if contractHash matches
  --output <dir>      Output directory (default: ./fixtures/acp-wire)
  --engine <v2|kas>   Engine (default: v2)
  --timeout <ms>      Per-scenario timeout (default: 120000)
  --dry-run           Show what would be recorded
  --help              Show this help
`);
    process.exit(0);
  }

  const engine = (values.engine as string) || 'v2';
  if (engine !== 'v2' && engine !== 'kas') {
    logError('args', `Invalid engine: "${engine}". Must be "v2" or "kas".`);
    process.exit(2);
  }

  return {
    scenario: values.scenario as string | undefined,
    category: values.category as string | undefined,
    all: values.all as boolean,
    check: values.check as boolean,
    force: values.force as boolean,
    output: values.output as string,
    engine: engine as Engine,
    timeout: parseInt(values.timeout as string, 10),
    dryRun: values['dry-run'] as boolean,
  };
}

// ---------------------------------------------------------------------------
// --check mode: report stale/missing fixtures without recording
// ---------------------------------------------------------------------------

function runCheck(scenarios: Scenario[], outputDir: string): number {
  let staleCount = 0;
  let missingCount = 0;
  let okCount = 0;

  for (const scenario of scenarios) {
    if (!needsFixture(scenario)) continue;

    const fp = fixturePath(outputDir, scenario);
    const fixture = loadFixture(fp);

    if (!fixture) {
      log('check', `MISSING  ${scenario.id}`);
      missingCount++;
      continue;
    }

    const currentHash = computeContractHash(scenario);
    if (fixture.contractHash !== currentHash) {
      log('check', `STALE    ${scenario.id} (contract changed)`);
      staleCount++;
    } else {
      log('check', `OK       ${scenario.id}`);
      okCount++;
    }
  }

  const eligible = scenarios.filter(needsFixture).length;
  log('check', `\n${okCount} up-to-date, ${staleCount} stale, ${missingCount} missing (${eligible} eligible)`);

  return staleCount > 0 || missingCount > 0 ? 4 : 0;
}

// ---------------------------------------------------------------------------
// Record mode
// ---------------------------------------------------------------------------

async function runRecord(scenarios: Scenario[], opts: CliOptions): Promise<number> {
  const eligible = scenarios.filter(needsFixture);
  const engineVersion = getEngineVersion();

  log('recorder', `${scenarios.length} scenarios loaded, ${eligible.length} need fixtures`);

  if (eligible.length === 0) {
    log('recorder', 'nothing to record (no prompt steps in matched scenarios)');
    return 0;
  }

  const results: RecordResult[] = [];

  for (const scenario of eligible) {
    const fp = fixturePath(opts.output, scenario);
    const existing = loadFixture(fp);
    const currentHash = computeContractHash(scenario);

    if (existing && existing.contractHash === currentHash && !opts.force) {
      results.push({ scenarioId: scenario.id, status: 'up-to-date' });
      continue;
    }

    const reason = !existing ? 'missing' : 'stale';

    if (opts.dryRun) {
      log('dry-run', `would record: ${scenario.id} (${reason})`);
      results.push({ scenarioId: scenario.id, status: reason });
      continue;
    }

    log('recorder', `recording ${scenario.id} [${reason}] (engine=${opts.engine})...`);

    try {
      const prompts = await recordScenario(scenario, {
        engine: opts.engine,
        timeout: opts.timeout,
      });

      const fixture: ScenarioFixture = {
        schemaVersion: 1,
        scenarioId: scenario.id,
        recordedAt: new Date().toISOString(),
        engine: opts.engine,
        engineVersion,
        contractHash: currentHash,
        prompts,
      };

      writeFixtureAtomic(fp, fixture);
      log('recorder', `  wrote ${fp} (hash: ${currentHash.slice(0, 12)}...)`);
      results.push({ scenarioId: scenario.id, status: 'recorded', fixture });
    } catch (err: any) {
      logError('recorder', `  FAILED: ${err.message}`);
      results.push({ scenarioId: scenario.id, status: 'failed', error: err.message });
    }
  }

  // Summary
  const recorded = results.filter((r) => r.status === 'recorded').length;
  const failed = results.filter((r) => r.status === 'failed').length;
  const upToDate = results.filter((r) => r.status === 'up-to-date').length;
  const skipped = scenarios.length - eligible.length;

  log('recorder', `\ndone: ${recorded} recorded, ${failed} failed, ${upToDate} up-to-date, ${skipped} ineligible`);

  if (failed > 0) {
    logError('recorder', 'failed scenarios:');
    for (const r of results.filter((r) => r.status === 'failed')) {
      logError('recorder', `  - ${r.scenarioId}: ${r.error}`);
    }
    return 1;
  }

  return 0;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const opts = parseCliArgs();

  if (opts.engine === 'kas') {
    logError('recorder', 'KAS engine recording is not yet supported. Use --engine v2.');
    process.exit(2);
  }

  const allScenarios = loadScenarios();

  let scenarios: Scenario[];
  if (opts.all) {
    scenarios = allScenarios;
  } else {
    scenarios = filterScenarios(allScenarios, {
      backend: {
        ...FILTER_BACKEND,
        engine: opts.engine,
      },
      scenarios: opts.scenario ? [opts.scenario] : undefined,
      categories: opts.category ? [opts.category] : undefined,
    });
  }

  if (scenarios.length === 0) {
    logError('recorder', 'no scenarios matched the given filters');
    process.exit(2);
  }

  if (opts.check) {
    const code = runCheck(scenarios, opts.output);
    process.exit(code);
  }

  const code = await runRecord(scenarios, opts);
  process.exit(code);
}

main().catch((err) => {
  logError('recorder', `fatal: ${err.message}`);
  process.exit(1);
});
