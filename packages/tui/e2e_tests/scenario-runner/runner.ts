import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { assertVerify } from './predicates';
import { executeStep, frameLabel, isFrameStep } from './steps';

/**
 * When the suite runs under coverage, give each scenario its own lcov output
 * dir so sequential scenarios don't overwrite one shared file. CI sets
 * KIRO_COVERAGE=1 + KIRO_SCENARIO_COVERAGE_DIR (the base); this derives the
 * per-scenario KIRO_COVERAGE_DIR that both the acp-mock (TestCase) and krs-mock
 * (Rust launcher) paths read, plus the wrapper path the Rust launcher needs.
 * A no-op when coverage is off, so normal runs are unaffected.
 */
function prepareScenarioCoverageDir(scenarioId: string): void {
  if (process.env.KIRO_COVERAGE !== '1') return;
  const base = process.env.KIRO_SCENARIO_COVERAGE_DIR;
  if (!base) return;
  const dir = path.join(base, scenarioId);
  fs.mkdirSync(dir, { recursive: true });
  process.env.KIRO_COVERAGE_DIR = dir;
  process.env.KIRO_COVERAGE_WRAPPER = path.resolve(
    import.meta.dir,
    '../../src/test-utils/coverage-wrapper.ts'
  );
}
import type {
  Frame,
  RunOptions,
  RunReport,
  Scenario,
  ScenarioBackend,
  ScenarioBackendId,
  ScenarioResult,
  StepTiming,
  TestHarness,
} from './types';

const DEFAULT_BOOT_TIMEOUT = 30_000;
const DEFAULT_STEP_TIMEOUT = 45_000;
const STEP_SETTLE_DELAY = 200;

interface ScenarioRuntimeContext {
  scenarioId: string;
  tempDir: string;
}

function log(prefix: string, message: string): void {
  console.log(`[${prefix}] ${message}`);
}

function createScenarioRuntimeContext(scenarioId: string): ScenarioRuntimeContext {
  return {
    scenarioId,
    tempDir: fs.mkdtempSync(
      path.join(os.tmpdir(), `kiro-scenario-${scenarioId}-`)
    ),
  };
}

function cleanupScenarioRuntimeContext(ctx: ScenarioRuntimeContext): void {
  fs.rmSync(ctx.tempDir, { recursive: true, force: true });
}

function interpolateScenarioValue(
  value: string,
  ctx: ScenarioRuntimeContext
): string {
  return value
    .replaceAll('{{tmpDir}}', ctx.tempDir)
    .replaceAll('{{scenarioId}}', ctx.scenarioId)
    .replace(/\{\{tmpFile:([^}]+)\}\}/g, (_, fileName: string) =>
      path.join(ctx.tempDir, fileName)
    );
}

function captureFrame(harness: TestHarness, label: string): Frame {
  return {
    label,
    timestamp: Date.now(),
    text: harness.getSnapshot(),
    html: harness.getSnapshotHtml(),
  };
}

const DEFAULT_SCENARIOS_ROOT = path.join(__dirname, '../smoke/scenarios');

/** Scenarios here are portable: the lane's backend decides how they run. */
const SHARED_DIR = 'shared';

/** Every other directory names the one backend its scenarios run under. */
const BACKEND_DIRS: Record<string, ScenarioBackendId> = {
  live: 'live',
  'acp-mock': 'acp-mock',
  'krs-mock': 'krs-mock',
};

function readManifest(filePath: string): Scenario[] {
  const raw = JSON.parse(fs.readFileSync(filePath, 'utf-8'));
  const scenarios: Scenario[] = raw?.scenarios;
  if (!Array.isArray(scenarios)) {
    throw new Error(`Invalid scenario manifest ${filePath}: expected a "scenarios" array`);
  }

  for (const scenario of scenarios) {
    if (
      !scenario?.id ||
      !scenario.name ||
      !scenario.category ||
      !Array.isArray(scenario.steps) ||
      !Array.isArray(scenario.verify)
    ) {
      throw new Error(
        `Invalid scenario "${scenario?.id ?? '<unknown>'}" in ${filePath}: missing required fields (id, name, category, steps, verify)`
      );
    }
  }

  return scenarios;
}

function manifestPaths(dir: string): string[] {
  return fs
    .readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith('.json'))
    .map((entry) => path.join(dir, entry.name))
    .sort();
}

/**
 * Loads every scenario, tagging each with the backend its location implies.
 *
 * A path to a single file is read as one manifest whose scenarios are portable,
 * which is what an ad-hoc or generated manifest wants.
 */
export function loadScenarios(scenariosPath?: string): Scenario[] {
  const target = scenariosPath ?? DEFAULT_SCENARIOS_ROOT;
  if (!fs.existsSync(target)) {
    throw new Error(`Scenarios not found: ${target}`);
  }
  if (fs.statSync(target).isFile()) {
    return readManifest(target);
  }

  const loaded: Scenario[] = [];
  const seen = new Map<string, string>();

  for (const entry of fs.readdirSync(target, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;

    const backendId =
      entry.name === SHARED_DIR ? undefined : BACKEND_DIRS[entry.name];
    if (entry.name !== SHARED_DIR && !backendId) {
      throw new Error(
        `Unknown scenario directory "${entry.name}" in ${target}: expected ` +
          `"${SHARED_DIR}" or one of ${Object.keys(BACKEND_DIRS)
            .map((name) => `"${name}"`)
            .join(', ')}`
      );
    }

    for (const manifest of manifestPaths(path.join(target, entry.name))) {
      for (const scenario of readManifest(manifest)) {
        const previous = seen.get(scenario.id);
        if (previous) {
          throw new Error(
            `Duplicate scenario id "${scenario.id}" in ${manifest} (already defined in ${previous})`
          );
        }
        seen.set(scenario.id, manifest);
        loaded.push(backendId ? { ...scenario, sourceBackend: backendId } : scenario);
      }
    }
  }

  return loaded;
}

/** The id of the backend a scenario runs under, needing no backend instance. */
export function scenarioBackendId(
  scenario: Scenario,
  opts: RunOptions
): ScenarioBackendId {
  return scenario.sourceBackend ?? opts.backend.id;
}

/**
 * The backend a scenario runs under: its directory's, or the lane's when the
 * scenario is portable.
 */
export function resolveScenarioBackend(
  scenario: Scenario,
  opts: RunOptions
): ScenarioBackend {
  const id = scenarioBackendId(scenario, opts);
  if (id === opts.backend.id) {
    return opts.backend;
  }

  const backend = opts.resolveBackend?.(id);
  if (!backend) {
    throw new Error(
      `scenario "${scenario.id}" requires backend "${id}", ` +
        'which this runner was not given'
    );
  }
  return backend;
}

export function filterScenarios(
  scenarios: Scenario[],
  opts: RunOptions
): Scenario[] {
  let filtered = scenarios.filter((scenario) => {
    if (!scenario.engine || scenario.engine.length === 0) return true;
    return scenario.engine.includes(resolveScenarioBackend(scenario, opts).engine);
  });

  // A scenario is only eligible for the krs-mock lane if it says what the model
  // replies. Skipping the rest keeps the whole suite runnable on that lane.
  filtered = filtered.filter((scenario) => {
    if (scenarioBackendId(scenario, opts) !== 'krs-mock') return true;
    return !!scenario.turns && scenario.turns.length > 0;
  });

  // Asking for one backend means only that backend runs, so a lane stays as
  // deterministic (or as live) as it was asked to be. Without the request every
  // scenario runs, each under the backend its location names.
  if (opts.laneOnly) {
    filtered = filtered.filter(
      (scenario) => scenarioBackendId(scenario, opts) === opts.backend.id
    );
  }

  if (opts.scenarios && opts.scenarios.length > 0) {
    const ids = new Set(opts.scenarios);
    filtered = filtered.filter((scenario) => ids.has(scenario.id));
  }

  if (opts.categories && opts.categories.length > 0) {
    const categories = new Set(opts.categories);
    filtered = filtered.filter((scenario) => categories.has(scenario.category));
  }

  if (opts.tags && opts.tags.length > 0) {
    const tags = new Set(opts.tags);
    filtered = filtered.filter((scenario) => scenario.tags?.some((tag) => tags.has(tag)) ?? false);
  }

  if (opts.priority && opts.priority.length > 0) {
    const priorities = new Set(opts.priority);
    filtered = filtered.filter(
      (scenario) => !!scenario.priority && priorities.has(scenario.priority)
    );
  }

  return filtered;
}

// LINT-DEBT(sonarjs/cognitive-complexity): pre-existing at gate adoption; Refactor this function to reduce its Cognitive Complexity from 38 to the 30 allowed.; refactor before extending
// eslint-disable-next-line sonarjs/cognitive-complexity
export async function runScenario(
  scenario: Scenario,
  opts: RunOptions
): Promise<ScenarioResult> {
  const scenarioLog = `scenario:${scenario.id}`;
  const startTime = Date.now();
  const frames: Frame[] = [];
  const captureFrames = opts.captureFrames !== false;
  const timeout = opts.timeout ?? scenario.timeout ?? 120_000;
  const stepTimings: StepTiming[] = [];
  let harness: TestHarness | null = null;
  const runtime = createScenarioRuntimeContext(scenario.id);
  let initialSessionId: string | undefined;
  const backend = resolveScenarioBackend(scenario, opts);

  log(
    scenarioLog,
    `starting (backend=${backend.id}, engine=${backend.engine}, timeout=${timeout}ms)`
  );

  try {
    harness = await backend.launch(scenario, opts);
    await harness.waitForText('ask a question', DEFAULT_BOOT_TIMEOUT);
    initialSessionId = (await harness.getStore()).sessionId ?? undefined;

    if (captureFrames) {
      frames.push(captureFrame(harness, 'after-boot'));
    }

    for (let index = 0; index < scenario.steps.length; index++) {
      const step = interpolateScenarioValue(scenario.steps[index]!, runtime);
      const startedAt = Date.now();
      log(scenarioLog, `step ${index + 1}/${scenario.steps.length}: ${step}`);

      if (isFrameStep(step)) {
        if (captureFrames) {
          frames.push(captureFrame(harness, frameLabel(step)));
        }
        stepTimings.push({
          step,
          index,
          startedAt,
          duration: Date.now() - startedAt,
          status: 'done',
        });
      } else {
        try {
          await Promise.race([
            executeStep(harness, step),
            new Promise<never>((_, reject) =>
              setTimeout(
                () =>
                  reject(
                    new Error(
                      `Step timed out after ${DEFAULT_STEP_TIMEOUT}ms: "${step}"`
                    )
                  ),
                DEFAULT_STEP_TIMEOUT
              )
            ),
          ]);
          stepTimings.push({
            step,
            index,
            startedAt,
            duration: Date.now() - startedAt,
            status: 'done',
          });
        } catch (err) {
          const message = err instanceof Error ? err.message : String(err);
          stepTimings.push({
            step,
            index,
            startedAt,
            duration: Date.now() - startedAt,
            status: 'error',
            error: message,
          });
          throw err;
        }
      }

      await harness.sleepMs(STEP_SETTLE_DELAY);
    }

    const verifyResults = [];
    for (const rawPredicate of scenario.verify) {
      const predicate = interpolateScenarioValue(rawPredicate, runtime);
      const result = await assertVerify(harness, predicate, {
        initialSessionId,
      });
      verifyResults.push(result);
      if (!result.passed) {
        log(scenarioLog, `FAIL: ${predicate} — ${result.actual}`);
      }
    }

    for (const result of (await harness.selfChecks?.()) ?? []) {
      verifyResults.push(result);
      if (!result.passed) {
        log(scenarioLog, `FAIL: ${result.predicate} — ${result.actual}`);
      }
    }

    if (captureFrames) {
      frames.push(captureFrame(harness, 'final'));
    }

    const passed = verifyResults.every((result) => result.passed);
    const duration = Date.now() - startTime;

    return {
      scenario,
      backendId: backend.id,
      engine: backend.engine,
      passed,
      duration,
      exitReason: passed ? 'passed' : 'assertion-failed',
      verifyResults,
      stepTimings,
      frames,
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const normalizedMessage = message.toLowerCase();
    if (captureFrames && harness) {
      try {
        frames.push(captureFrame(harness, 'error'));
      } catch {
        // Ignore snapshot failures during teardown.
      }
    }

    let exitReason: ScenarioResult['exitReason'] = 'crash';
    if (message.includes('timed out') || message.includes('Timeout')) {
      exitReason = 'timeout';
    } else if (normalizedMessage.includes('fixture')) {
      exitReason = 'fixture-missing';
    }

    return {
      scenario,
      backendId: backend.id,
      engine: backend.engine,
      passed: false,
      duration: Date.now() - startTime,
      exitReason,
      verifyResults: [],
      stepTimings,
      error: message,
      frames,
    };
  } finally {
    if (harness) {
      try {
        await harness.cleanup();
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        log(scenarioLog, `cleanup warning: ${message}`);
      }
    }
    cleanupScenarioRuntimeContext(runtime);
  }
}

export async function runAll(opts: RunOptions): Promise<RunReport> {
  const startedAt = Date.now();
  log(
    'runner',
    `starting scenario run (backend=${opts.backend.id}, engine=${opts.backend.engine})`
  );

  const allScenarios = loadScenarios(opts.scenariosPath);
  const scenarios = filterScenarios(allScenarios, opts);
  const selectedScenarioIds = new Set(scenarios.map((scenario) => scenario.id));
  const skippedScenarioIds = allScenarios
    .filter((scenario) => !selectedScenarioIds.has(scenario.id))
    .map((scenario) => scenario.id);
  const results: ScenarioResult[] = [];
  const skipped = skippedScenarioIds.length;

  for (const scenario of scenarios) {
    prepareScenarioCoverageDir(scenario.id);
    results.push(await runScenario(scenario, opts));
  }

  const completedAt = Date.now();
  const passed = results.filter((result) => result.passed).length;
  const failed = results.filter((result) => !result.passed).length;

  // Named from what ran, not from the lane: in unpinned mode the lane backend is
  // only the fallback for shared scenarios, so using it would attribute a
  // pinned scenario's failure to a backend it never touched.
  const label = (values: string[], fallback: string) =>
    [...new Set(values)].sort().join('+') || fallback;
  const backendId = label(
    results.map((result) => result.backendId),
    opts.backend.id
  );
  const engine = label(
    results.map((result) => result.engine),
    opts.backend.engine
  );

  const report: RunReport = {
    backendId,
    engine,
    startedAt,
    completedAt,
    total: allScenarios.length,
    passed,
    failed,
    skipped,
    skippedScenarioIds,
    results,
  };

  if (opts.outputDir) {
    fs.mkdirSync(opts.outputDir, { recursive: true });
    const reportPath = path.join(
      opts.outputDir,
      `scenario-report-${backendId}-${startedAt}.json`
    );
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2));
    log('runner', `report written to ${reportPath}`);
  }

  log('runner', `completed in ${completedAt - startedAt}ms`);
  log(
    'runner',
    `results: ${passed} passed, ${failed} failed, ${skipped} skipped (${allScenarios.length} total)`
  );

  return report;
}
