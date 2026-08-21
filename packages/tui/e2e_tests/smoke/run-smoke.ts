#!/usr/bin/env bun
/**
 * Smoke test CLI entrypoint.
 *
 * Loads scenarios, runs them via scenario-runner, and emits results as either
 * TAP (for CI) or a human-friendly summary table. Exit code reflects the
 * highest-severity failure observed.
 *
 * Exit codes:
 *   0 = all passed
 *   1 = assertion-failed
 *   2 = timeout
 *   3 = crash
 *   4 = fixture-missing
 *   5 = runner error (CLI misuse or unhandled exception)
 */

import { parseArgs } from 'node:util';
import * as path from 'node:path';
import {
  type ExitReason,
  type RunReport,
  type RunOptions,
  type ScenarioResult,
  type ScenarioBackendId,
  type Engine,
  runAll,
} from './scenario-runner';
import { emitFailureContext } from './failure-context';
import { createAcpMockBackend } from '../scenario-runner/backends/acp-mock';
import { createKrsMockBackend } from '../scenario-runner/backends/krs-mock';
import { createLiveBackend } from '../scenario-runner/backends/live';

// ---------------------------------------------------------------------------
// Exit code mapping
// ---------------------------------------------------------------------------

const EXIT_CODE: Record<ExitReason, number> = {
  passed: 0,
  'assertion-failed': 1,
  timeout: 2,
  crash: 3,
  'fixture-missing': 4,
};

const RUNNER_ERROR = 5;

function highestExitCode(results: ScenarioResult[]): number {
  let max = 0;
  for (const r of results) {
    const code = EXIT_CODE[r.exitReason] ?? RUNNER_ERROR;
    if (code > max) max = code;
  }
  return max;
}

// ---------------------------------------------------------------------------
// CLI argument parsing
// ---------------------------------------------------------------------------

interface CliOptions {
  backend: ScenarioBackendId;
  backendExplicit: boolean;
  engine: Engine;
  model: string | undefined;
  fixturesDir: string;
  format: 'tap' | 'summary';
  scenarios: string[];
  categories: string[];
  tags: string[];
  priority: string[];
  timeout: number;
  timeoutExplicit: boolean;
  captureFrames: boolean;
  outputDir: string | undefined;
  retries: number;
  ci: boolean;
  help: boolean;
}

// LINT-DEBT(complexity): pre-existing at gate adoption; Function 'parseCliArgs' has a complexity of 35. Maximum allowed is 30.; refactor before extending
// eslint-disable-next-line complexity
function parseCliArgs(): CliOptions {
  const { values } = parseArgs({
    options: {
      backend: { type: 'string', short: 'b', default: undefined },
      engine: { type: 'string', short: 'e', default: undefined },
      mode: { type: 'string', short: 'm', default: undefined },
      'fixtures-dir': { type: 'string', default: undefined },
      format: { type: 'string', short: 'f', default: undefined },
      scenario: { type: 'string', short: 's', multiple: true, default: [] },
      category: { type: 'string', short: 'c', multiple: true, default: [] },
      tag: { type: 'string', short: 't', multiple: true, default: [] },
      priority: { type: 'string', short: 'p', multiple: true, default: [] },
      model: { type: 'string', default: undefined },
      timeout: { type: 'string', default: undefined },
      'capture-frames': { type: 'boolean', default: undefined },
      'no-capture-frames': { type: 'boolean', default: false },
      'output-dir': { type: 'string', short: 'o', default: undefined },
      retries: { type: 'string', default: undefined },
      ci: { type: 'boolean', default: false },
      help: { type: 'boolean', short: 'h', default: false },
    },
    strict: true,
    allowPositionals: false,
  });

  const ci = values.ci ?? false;

  // Env var precedence: explicit flag > env var > default
  const envBackend = process.env.SMOKE_BACKEND as ScenarioBackendId | undefined;
  const envEngine = process.env.SMOKE_ENGINE as Engine | undefined;
  const envFormat = process.env.SMOKE_FORMAT as 'tap' | 'summary' | undefined;
  const envTimeout = process.env.SMOKE_TIMEOUT;
  const envOutputDir = process.env.SMOKE_OUTPUT_DIR;
  const envRetries = process.env.SMOKE_RETRIES;

  const legacyMode = ((values.mode as string) ?? process.env.SMOKE_MODE) as
    | 'live'
    | 'deterministic'
    | undefined;
  const backend: ScenarioBackendId =
    (values.backend as ScenarioBackendId) ??
    envBackend ??
    (legacyMode === 'deterministic' ? 'acp-mock' : 'live');
  const backendExplicit = !!(values.backend || envBackend || legacyMode);
  const defaultEngine: Engine = 'kas';
  const engine: Engine = (values.engine as Engine) ?? envEngine ?? defaultEngine;
  const format: 'tap' | 'summary' =
    (values.format as 'tap' | 'summary') ?? envFormat ?? (ci ? 'tap' : 'summary');
  const timeout = values.timeout ? parseInt(values.timeout, 10) : envTimeout ? parseInt(envTimeout, 10) : 120_000;
  const timeoutExplicit = !!(values.timeout || envTimeout);
  const retries = values.retries ? parseInt(values.retries, 10) : envRetries ? parseInt(envRetries, 10) : (ci ? 1 : 0);
  const outputDir = values['output-dir'] ?? envOutputDir ?? (ci ? './smoke-results' : undefined);

  // capture-frames: explicit flag wins, then CI default (off), then true
  let captureFrames: boolean;
  if (values['no-capture-frames']) {
    captureFrames = false;
  } else if (values['capture-frames'] !== undefined) {
    captureFrames = values['capture-frames'];
  } else {
    captureFrames = ci ? false : true;
  }

  const BACKENDS: ScenarioBackendId[] = ['live', 'acp-mock', 'krs-mock'];
  if (!BACKENDS.includes(backend)) {
    console.error(
      `error: invalid backend "${backend}" (expected ${BACKENDS.map((id) => `"${id}"`).join(', ')})`
    );
    process.exit(RUNNER_ERROR);
  }

  if (engine !== 'v2' && engine !== 'kas') {
    console.error(`error: invalid engine "${engine}" (expected "v2" or "kas")`);
    process.exit(RUNNER_ERROR);
  }

  // Neither mock backend has a v2 path: `acp-mock` replays KAS-shaped ACP
  // fixtures, and only KAS talks to KRS.
  if (backend !== 'live' && engine !== 'kas') {
    console.error(`error: backend "${backend}" only supports engine "kas"`);
    process.exit(RUNNER_ERROR);
  }

  const model = (values.model as string) ?? process.env.SMOKE_MODEL;

  // Silently ignoring this would report a pass for a model that never ran.
  if (model && backend !== 'live') {
    console.error(
      `error: --model requires backend "live" (got "${backend}"); the mocks replay fixtures and never reach a model`
    );
    process.exit(RUNNER_ERROR);
  }

  return {
    backend,
    backendExplicit,
    engine,
    model,
    fixturesDir:
      (values['fixtures-dir'] as string) ??
      process.env.SMOKE_FIXTURES_DIR ??
      path.join(__dirname, 'fixtures', 'acp-wire'),
    format,
    scenarios: (values.scenario as string[]) ?? [],
    categories: (values.category as string[]) ?? [],
    tags: (values.tag as string[]) ?? [],
    priority: (values.priority as string[]) ?? [],
    timeout,
    timeoutExplicit,
    captureFrames,
    outputDir,
    retries,
    ci,
    help: values.help ?? false,
  };
}

// ---------------------------------------------------------------------------
// TAP output
// ---------------------------------------------------------------------------

// LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 44 to the 30 allowed.; refactor before extending
// eslint-disable-next-line sonarjs/cognitive-complexity
function emitTap(report: RunReport): void {
  console.log('TAP version 14');
  console.log(`1..${report.results.length}`);
  for (const id of report.skippedScenarioIds ?? []) {
    console.log(`# skipped by filter/runtime: ${id}`);
  }

  let num = 0;
  for (const r of report.results) {
    num++;
    const durationSec = (r.duration / 1000).toFixed(2);
    const desc = `${r.scenario.id} - ${r.scenario.name} (${durationSec}s)`;

    if (r.passed) {
      console.log(`ok ${num} - ${desc}`);
    } else {
      console.log(`not ok ${num} - ${desc}`);
      // YAML diagnostic block
      console.log('  ---');
      console.log(`  exitReason: ${r.exitReason}`);
      console.log(`  backend: ${r.backendId}`);
      console.log(`  engine: ${r.engine}`);
      console.log(`  duration_ms: ${r.duration}`);
      if (r.error) {
        console.log(`  error: "${r.error.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`);
      }
      if (r.verifyResults.length > 0) {
        const failures = r.verifyResults.filter((v) => !v.passed);
        if (failures.length > 0) {
          console.log('  failures:');
          for (const f of failures) {
            console.log(`    - predicate: "${f.predicate}"`);
            // LINT-DEBT(max-depth): pre-existing at gate adoption; Blocks are nested too deeply (6). Maximum allowed is 5.; refactor before extending
            // eslint-disable-next-line max-depth
            if (f.actual) {
              console.log(`      actual: "${f.actual.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`);
            }
          }
        }
      }
      if (r.stepTimings.length > 0) {
        const errSteps = r.stepTimings.filter((s) => s.status === 'error');
        if (errSteps.length > 0) {
          console.log('  failedSteps:');
          for (const s of errSteps) {
            console.log(`    - step: "${s.step}"`);
            console.log(`      index: ${s.index}`);
            // LINT-DEBT(max-depth): pre-existing at gate adoption; Blocks are nested too deeply (6). Maximum allowed is 5.; refactor before extending
            // eslint-disable-next-line max-depth
            if (s.error) console.log(`      error: "${s.error.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`);
          }
        }
      }
      console.log('  ...');
    }
  }
}

// ---------------------------------------------------------------------------
// Summary table output
// ---------------------------------------------------------------------------

function emitSummary(report: RunReport): void {
  const totalDuration = ((report.completedAt - report.startedAt) / 1000).toFixed(1);

  // A run can span backends, so the header names the ones that actually ran
  // rather than the lane's, which would mislabel every pinned scenario.
  const used = [...new Set(report.results.map((r) => `${r.backendId}/${r.engine}`))].sort();

  console.log('');
  console.log(`Smoke Test Results (${used.join(', ') || 'no scenarios'})`);
  console.log('='.repeat(70));
  console.log('');

  // Results table
  const STATUS_WIDTH = 6;
  const ID_WIDTH = 32;
  const BACKEND_WIDTH = 9;
  const DURATION_WIDTH = 8;
  const REASON_WIDTH = 18;

  const header = [
    'Status'.padEnd(STATUS_WIDTH),
    'Scenario'.padEnd(ID_WIDTH),
    'Backend'.padEnd(BACKEND_WIDTH),
    'Duration'.padEnd(DURATION_WIDTH),
    'Reason'.padEnd(REASON_WIDTH),
  ].join(' | ');

  console.log(header);
  console.log('-'.repeat(header.length));

  for (const r of report.results) {
    const status = r.passed ? '\x1b[32mPASS\x1b[0m  ' : '\x1b[31mFAIL\x1b[0m  ';
    const id = r.scenario.id.padEnd(ID_WIDTH).slice(0, ID_WIDTH);
    const backend = r.backendId.padEnd(BACKEND_WIDTH).slice(0, BACKEND_WIDTH);
    const dur = `${(r.duration / 1000).toFixed(1)}s`.padEnd(DURATION_WIDTH);
    const reason = r.exitReason.padEnd(REASON_WIDTH);
    console.log(`${status}| ${id} | ${backend} | ${dur} | ${reason}`);
  }

  console.log('');
  console.log('-'.repeat(70));
  console.log(
    `Total: ${report.total}  |  ` +
      `\x1b[32mPassed: ${report.passed}\x1b[0m  |  ` +
      `\x1b[31mFailed: ${report.failed}\x1b[0m  |  ` +
      `Skipped: ${report.skipped}  |  ` +
      `Time: ${totalDuration}s`
  );
  console.log('');
  if ((report.skippedScenarioIds?.length ?? 0) > 0) {
    console.log(`Skipped IDs: ${report.skippedScenarioIds!.join(', ')}`);
    console.log('');
  }

  // Show failures detail
  if (report.failed > 0) {
    console.log('Failures:');
    console.log('');
    for (const r of report.results.filter((r) => !r.passed)) {
      console.log(
        `  \x1b[31m✗\x1b[0m ${r.scenario.id} (${r.exitReason}, backend=${r.backendId})`
      );
      if (r.error) {
        console.log(`    error: ${r.error}`);
      }
      for (const v of r.verifyResults.filter((v) => !v.passed)) {
        console.log(`    - ${v.predicate}: ${v.actual}`);
      }
    }
    console.log('');
  }
}

// ---------------------------------------------------------------------------
// Retry logic
// ---------------------------------------------------------------------------

async function runWithRetries(opts: RunOptions, retries: number): Promise<RunReport> {
  const report = await runAll(opts);

  if (retries <= 0 || report.failed === 0) {
    return report;
  }

  // Retry only failed scenarios
  const failedIds = report.results.filter((r) => !r.passed).map((r) => r.scenario.id);
  console.error(`\nRetrying ${failedIds.length} failed scenario(s) (retries left: ${retries})...\n`);

  const retryOpts: RunOptions = {
    ...opts,
    scenarios: failedIds,
  };

  const retryReport = await runWithRetries(retryOpts, retries - 1);

  // Merge: replace failed results with retry results where they passed
  const retryMap = new Map(retryReport.results.map((r) => [r.scenario.id, r]));
  const mergedResults: ScenarioResult[] = report.results.map((r) => {
    const retried = retryMap.get(r.scenario.id);
    if (!r.passed && retried) {
      return retried;
    }
    return r;
  });

  return {
    backendId: report.backendId,
    engine: report.engine,
    startedAt: report.startedAt,
    completedAt: Date.now(),
    total: report.total,
    passed: mergedResults.filter((r) => r.passed).length,
    failed: mergedResults.filter((r) => !r.passed).length,
    skipped: report.skipped,
    skippedScenarioIds: report.skippedScenarioIds,
    results: mergedResults,
  };
}

// ---------------------------------------------------------------------------
// Help text
// ---------------------------------------------------------------------------

function printHelp(): void {
  console.log(`
Usage: run-smoke [options]

Run smoke test scenarios against the Kiro CLI TUI.

A scenario's directory under e2e_tests/smoke/scenarios/ says which backend runs
it: "shared" is portable and runs under whichever backend the lane selects, and
any other directory names the one backend its scenarios run under. With no
--backend every scenario runs, each under the backend its location names; with
--backend only that backend's scenarios run.

Options:
  -b, --backend <live|acp-mock|krs-mock>
                              Run only this backend's scenarios plus the shared
                              ones (env: SMOKE_BACKEND)
                              live     real services
                              acp-mock replay a recorded ACP-wire fixture
                              krs-mock real KAS against the fake Kiro Runtime
                                       Service; only scenarios carrying turns
                                       (needs: cargo build -p mock-krs-server)
  -e, --engine <v2|kas>       Agent engine for the selected backend (default: kas, env: SMOKE_ENGINE)
  -m, --mode <live|determ.>   Legacy alias: live -> backend=live, determ. -> backend=acp-mock
      --fixtures-dir <path>   ACP mock fixture directory (env: SMOKE_FIXTURES_DIR)
  -f, --format <tap|summary>  Output format (default: summary, env: SMOKE_FORMAT)
  -s, --scenario <id>         Run specific scenario(s) by ID (repeatable)
  -c, --category <name>       Filter by category (repeatable)
  -t, --tag <tag>             Filter by tag (repeatable)
  -p, --priority <p0|p1|p2>   Filter by priority (repeatable)
      --model <id>            Pin every scenario to this model id; requires
                              --backend live (env: SMOKE_MODEL)
      --timeout <ms>          Per-scenario timeout in ms (default: 120000, env: SMOKE_TIMEOUT)
      --capture-frames        Capture terminal frames as evidence (default: true)
      --no-capture-frames     Disable frame capture
  -o, --output-dir <path>     Write JSON report to directory (env: SMOKE_OUTPUT_DIR)
      --retries <n>           Retry failed scenarios n times (default: 0, env: SMOKE_RETRIES)
      --ci                    CI mode: format=tap, no-capture-frames, retries=1
  -h, --help                  Show this help

Exit codes:
  0  All scenarios passed
  1  One or more assertions failed
  2  One or more scenarios timed out
  3  One or more scenarios crashed
  4  One or more fixture files missing
  5  Runner error (CLI misuse, unhandled exception)

Examples:
  bun run-smoke.ts --ci --backend acp-mock
  bun run-smoke.ts --backend live --engine kas
  bun run-smoke.ts -s boot-prompt -s help-command
  bun run-smoke.ts --category navigation --format tap
  bun run-smoke.ts --tag regression --priority p0 -o ./smoke-results
`);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function main(): Promise<void> {
  const cli = parseCliArgs();

  if (cli.help) {
    printHelp();
    process.exit(0);
  }

  const createBackend = (id: ScenarioBackendId) =>
    id === 'acp-mock'
      ? createAcpMockBackend(cli.engine)
      : id === 'krs-mock'
        ? createKrsMockBackend('kas')
        : createLiveBackend(cli.engine);

  const runOpts: RunOptions = {
    backend: createBackend(cli.backend),
    resolveBackend: createBackend,
    laneOnly: cli.backendExplicit,
    fixturesDir: cli.fixturesDir,
    scenarios: cli.scenarios.length > 0 ? cli.scenarios : undefined,
    categories: cli.categories.length > 0 ? cli.categories : undefined,
    tags: cli.tags.length > 0 ? cli.tags : undefined,
    priority: cli.priority.length > 0 ? cli.priority : undefined,
    timeout: cli.timeoutExplicit ? cli.timeout : undefined,
    captureFrames: cli.captureFrames,
    outputDir: cli.outputDir,
    model: cli.model,
  };

  const report = await runWithRetries(runOpts, cli.retries);

  // Emit failure context files for failed scenarios
  if (cli.outputDir && report.failed > 0) {
    for (const result of report.results) {
      emitFailureContext(result, cli.outputDir);
    }
  }

  // Emit output
  if (cli.format === 'tap') {
    emitTap(report);
  } else {
    emitSummary(report);
  }

  // Exit code: highest severity across all results
  if (report.results.length === 0) {
    console.error('error: no scenarios matched the filter');
    process.exit(RUNNER_ERROR);
  }
  const exitCode = highestExitCode(report.results);
  process.exit(exitCode);
}

main().catch((err) => {
  console.error(`runner error: ${err.message}`);
  process.exit(RUNNER_ERROR);
});
