import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import type {
  ExitReason,
  Frame,
  ScenarioResult,
  StepTiming,
  VerifyResult,
} from './types';

export interface FailureContextJson {
  version: 2;
  scenario: {
    id: string;
    name: string;
    category: string;
    backendId: string;
    engine: string;
  };
  failure: {
    type: ExitReason;
    message: string;
    stepIndex: number | null;
    step: string | null;
  };
  timing: {
    startedAt: string;
    failedAt: string;
    durationMs: number;
  };
  screen: {
    lines: string[];
    dimensions: { width: number; height: number };
  };
  steps: Array<{
    index: number;
    step: string;
    status: 'passed' | 'failed' | 'skipped';
    elapsedMs: number;
    error?: string;
  }>;
  verifyResults: VerifyResult[];
  environment: Record<string, string>;
  triage: {
    type: ExitReason;
    commands: string[];
    hints: string[];
  };
}

function getLastFrame(frames: Frame[]): Frame | null {
  return frames.length === 0 ? null : (frames[frames.length - 1] ?? null);
}

function getFailedStep(timings: StepTiming[]): StepTiming | null {
  return timings.find((timing) => timing.status === 'error') ?? null;
}

function inferScreenDimensions(lines: string[]): { width: number; height: number } {
  const height = lines.length;
  const width = lines.reduce((max, line) => Math.max(max, line.length), 0);
  return { width: width || 120, height: height || 40 };
}

function getEnvironment(): Record<string, string> {
  return {
    os: `${os.platform()}-${os.arch()}`,
    bunVersion: typeof Bun !== 'undefined' ? Bun.version : process.version,
    terminalSize: `${process.stdout.columns ?? 120}x${process.stdout.rows ?? 40}`,
    KIRO_AGENT_ENGINE: process.env.KIRO_AGENT_ENGINE ?? 'unset',
    CI: process.env.CI ?? 'unset',
    GITHUB_RUN_ID: process.env.GITHUB_RUN_ID ?? 'unset',
    GITHUB_SHA: process.env.GITHUB_SHA ?? 'unset',
  };
}

function generateTriage(result: ScenarioResult): { commands: string[]; hints: string[] } {
  const id = result.scenario.id;
  const backend = result.backendId;
  const engine = result.engine;

  switch (result.exitReason) {
    case 'assertion-failed':
      return {
        commands: [
          `bun run knight-rider -- --scenario ${id} --backend ${backend}${backend === 'live' ? ` --engine ${engine}` : ''}`,
        ],
        hints: [
          'Check whether the expected UI text moved or changed.',
          'If this is an ACP mock scenario, verify the wire fixture still matches the scenario prompts.',
        ],
      };
    case 'timeout':
      return {
        commands: [
          `bun run e2e_tests/smoke/run-smoke.ts --scenario ${id} --backend ${backend}${backend === 'live' ? ` --engine ${engine}` : ''} --timeout 180000`,
        ],
        hints: [
          'Check whether the TUI reached the expected screen before the timeout.',
          'If the timeout happened on a prompt step, inspect the prompt fixture or mock handlers.',
        ],
      };
    case 'crash':
      return {
        commands: [
          `cat $TMPDIR/kiro-log/kiro-chat.log | tail -100`,
          `cat ~/.kiro/logs/*/kiro.log | tail -100`,
        ],
        hints: [
          'Check Rust logs first, then KAS logs if the live backend used engine=kas.',
          'Re-run the single scenario locally with frame capture enabled.',
        ],
      };
    case 'fixture-missing':
      return {
        commands: [
          `bun run e2e_tests/smoke/record-fixtures.ts --scenario ${id}`,
        ],
        hints: [
          'This scenario expected a recorded prompt fixture and none was available.',
          'Record or migrate the fixture before re-running the ACP mock backend.',
        ],
      };
    default:
      return {
        commands: [],
        hints: ['No triage suggestions for this failure type.'],
      };
  }
}

function buildFailureJson(result: ScenarioResult): FailureContextJson {
  const lastFrame = getLastFrame(result.frames);
  const failedStep = getFailedStep(result.stepTimings);
  const screenLines = lastFrame?.text ?? [];
  const now = Date.now();

  return {
    version: 2,
    scenario: {
      id: result.scenario.id,
      name: result.scenario.name,
      category: result.scenario.category,
      backendId: result.backendId,
      engine: result.engine,
    },
    failure: {
      type: result.exitReason,
      message: result.error ?? failedStep?.error ?? 'Unknown failure',
      stepIndex: failedStep?.index ?? null,
      step: failedStep?.step ?? null,
    },
    timing: {
      startedAt: new Date(now - result.duration).toISOString(),
      failedAt: new Date(now).toISOString(),
      durationMs: result.duration,
    },
    screen: {
      lines: screenLines,
      dimensions: inferScreenDimensions(screenLines),
    },
    steps: result.stepTimings.map((timing) => ({
      index: timing.index,
      step: timing.step,
      status: timing.status === 'error' ? 'failed' : 'passed',
      elapsedMs: timing.duration,
      error: timing.error,
    })),
    verifyResults: result.verifyResults,
    environment: getEnvironment(),
    triage: {
      type: result.exitReason,
      ...generateTriage(result),
    },
  };
}

function renderMarkdown(ctx: FailureContextJson): string {
  const lines: string[] = [];

  lines.push(`# Failure: ${ctx.scenario.name}`);
  lines.push('');
  lines.push('## Summary');
  lines.push('');
  lines.push('| Field | Value |');
  lines.push('|-------|-------|');
  lines.push(`| Scenario | \`${ctx.scenario.id}\` |`);
  lines.push(`| Backend | \`${ctx.scenario.backendId}\` |`);
  lines.push(`| Engine | \`${ctx.scenario.engine}\` |`);
  lines.push(`| Platform | \`${ctx.environment.os}\` |`);
  lines.push(`| Timestamp | \`${ctx.timing.failedAt}\` |`);
  lines.push(`| Duration | \`${ctx.timing.durationMs}ms\` |`);
  lines.push(`| Failure Type | \`${ctx.failure.type}\` |`);
  lines.push(`| Error | ${ctx.failure.message} |`);
  lines.push('');
  lines.push('## Screen State at Failure');
  lines.push('');
  if (ctx.screen.lines.length > 0) {
    lines.push('```text');
    lines.push(...ctx.screen.lines);
    lines.push('```');
  } else {
    lines.push('_No screen capture available._');
  }
  lines.push('');
  lines.push('## Triage');
  lines.push('');
  for (const command of ctx.triage.commands) {
    lines.push(`- \`${command}\``);
  }
  for (const hint of ctx.triage.hints) {
    lines.push(`- ${hint}`);
  }
  lines.push('');

  return lines.join('\n');
}

export function emitFailureContext(
  result: ScenarioResult,
  outputDir: string
): void {
  if (result.passed) return;

  const ctx = buildFailureJson(result);
  const baseName = `${result.scenario.id}-${result.backendId}-${Date.now()}`;
  fs.mkdirSync(outputDir, { recursive: true });
  fs.writeFileSync(
    path.join(outputDir, `${baseName}.json`),
    JSON.stringify(ctx, null, 2)
  );
  fs.writeFileSync(
    path.join(outputDir, `${baseName}.md`),
    renderMarkdown(ctx)
  );
}
