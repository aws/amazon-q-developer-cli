/**
 * Shared boilerplate for the lite-mode blackbox probes (forced-trim,
 * message-ordering, session-lifetime). Each probe keeps only its own
 * threshold/verification logic; everything below is identical ceremony.
 *
 * Exit codes (convention across all probes): 0 = pass, 1 = finding, 2 = crash.
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

export interface ProbeContext {
  name: string;
  platform: string;
  outputDir: string;
  /** `${name}-${platform}-${ts}` — prefix for all emitted files. */
  prefix: string;
  startedAt: number;
}

function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getUTCFullYear()}${pad(now.getUTCMonth() + 1)}${pad(now.getUTCDate())}-${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}`;
}

export function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 50);
}

export function createProbeContext(name: string): ProbeContext {
  const platform =
    process.env.KIRO_PROBE_PLATFORM ??
    (process.platform === 'darwin' ? 'macos' : process.platform);
  const outputDir = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
  mkdirSync(outputDir, { recursive: true });
  return {
    name,
    platform,
    outputDir,
    prefix: `${name}-${platform}-${timestamp()}`,
    startedAt: Date.now(),
  };
}

export function writeFinding(
  ctx: ProbeContext,
  opts: {
    slug: string;
    title: string;
    severity: 'crash' | 'spiral' | 'regression' | 'slowdown' | 'smell';
    description: string;
    evidence?: string;
    proposedFix?: string;
    /** Frontmatter metadata that differs per probe. */
    workItem: string;
    review: string;
    technique: string;
    file: string;
  }
): string {
  const findingId = `${ctx.prefix}-${slugify(opts.slug)}`;
  const filePath = join(ctx.outputDir, `${findingId}.md`);
  const frontmatter = [
    '---',
    `id: ${findingId}`,
    `work-item: ${opts.workItem}`,
    `review: ${opts.review}`,
    `technique: ${opts.technique}`,
    `class: ${ctx.name}`,
    `severity: ${opts.severity}`,
    `file: ${opts.file}`,
    `platforms-affected: [${ctx.platform}]`,
    `discovered-by: blackbox`,
    `discovered-at: ${new Date().toISOString()}`,
    `status: open`,
    '---',
  ].join('\n');
  const body = [
    `# ${opts.title}`,
    '',
    opts.description,
    opts.evidence ? `\n## Evidence\n\n${opts.evidence}` : '',
    opts.proposedFix ? `\n## Proposed fix\n\n${opts.proposedFix}` : '',
  ]
    .filter(Boolean)
    .join('\n');
  writeFileSync(filePath, `${frontmatter}\n\n${body}\n`);
  return filePath;
}

export function writeMetrics(
  ctx: ProbeContext,
  metrics: Record<string, unknown>
): void {
  writeFileSync(
    join(ctx.outputDir, `${ctx.prefix}-metrics.json`),
    JSON.stringify(metrics, null, 2)
  );
}

export function writeDoneMarker(
  ctx: ProbeContext,
  opts: { workItem: string; findingsEmitted: number; summary: string }
): void {
  const elapsedMs = Date.now() - ctx.startedAt;
  writeFileSync(
    join(ctx.outputDir, `${ctx.prefix}-done.md`),
    [
      '---',
      `id: ${ctx.prefix}-done`,
      `work-item: ${opts.workItem}`,
      `kind: blackbox`,
      `platform: ${ctx.platform}`,
      `status: done`,
      `findings-emitted: ${opts.findingsEmitted}`,
      `elapsed-ms: ${elapsedMs}`,
      `completed-at: ${new Date().toISOString()}`,
      '---',
      '',
      `# Probe done: ${ctx.name} on ${ctx.platform}`,
      '',
      opts.summary,
      '',
    ].join('\n')
  );
}

/**
 * Ordinary-least-squares slope of y over x for the given samples. Returns 0
 * for fewer than 2 points or a degenerate (zero-variance) x.
 */
export function linearSlope<T>(
  samples: T[],
  x: (s: T) => number,
  y: (s: T) => number
): number {
  if (samples.length < 2) return 0;
  const xs = samples.map(x);
  const ys = samples.map(y);
  const n = xs.length;
  const xMean = xs.reduce((a, b) => a + b, 0) / n;
  const yMean = ys.reduce((a, b) => a + b, 0) / n;
  let num = 0;
  let den = 0;
  for (let j = 0; j < n; j++) {
    num += (xs[j]! - xMean) * (ys[j]! - yMean);
    den += (xs[j]! - xMean) ** 2;
  }
  return den !== 0 ? num / den : 0;
}

/**
 * Top-level crash wrapper: runs `main`, and on a thrown error writes
 * `<prefix>-error.log` and exits with code 2. `main` is expected to call
 * `process.exit` itself for the pass/finding cases.
 */
export async function runProbe(
  ctx: ProbeContext,
  main: () => Promise<void>
): Promise<void> {
  try {
    await main();
  } catch (err) {
    mkdirSync(ctx.outputDir, { recursive: true });
    writeFileSync(
      join(ctx.outputDir, `${ctx.prefix}-error.log`),
      String(err instanceof Error ? (err.stack ?? err.message) : err)
    );
    console.error(`[${ctx.name}] probe crashed on ${ctx.platform}:`, err);
    process.exit(2);
  }
}
