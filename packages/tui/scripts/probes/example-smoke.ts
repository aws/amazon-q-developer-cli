#!/usr/bin/env bun
/**
 * example-smoke.ts — minimum viable blackbox probe.
 *
 * Demonstrates the probe contract:
 *   - reads KIRO_PROBE_PLATFORM (or infers from process.platform)
 *   - writes one file per finding to $PROBE_OUTPUT_DIR
 *   - writes a done-marker when the probe completes
 *   - emits metrics.json for machine parsing
 *   - prints a human-readable summary to stdout
 *   - exits 0 on "no finding", 1 on "finding detected", 2 on probe crash
 *
 * Finding file naming (matches the runner/findings/ convention):
 *
 *   <probe>-<platform>-<YYYYMMDD>-<HHMM>-<slug>.md         # one per finding
 *   <probe>-<platform>-<YYYYMMDD>-<HHMM>-done.md           # probe completed
 *   <probe>-<platform>-<YYYYMMDD>-<HHMM>-error.log         # probe crashed
 *
 * This probe does not actually do anything interesting. It exists so the
 * workflow can be exercised end-to-end on all three OSes to verify the
 * plumbing works. Copy this file when starting a real probe.
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/example-smoke.ts
 *
 * Or via the workflow:
 *   gh workflow run blackbox-probe.yml -f probe=example-smoke
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const PROBE_NAME = 'example-smoke';
const PLATFORM = process.env.KIRO_PROBE_PLATFORM ?? detectPlatform();
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';

function detectPlatform(): string {
  switch (process.platform) {
    case 'linux':
      return 'linux';
    case 'darwin':
      return 'macos';
    case 'win32':
      return 'windows';
    default:
      return process.platform;
  }
}

/** Build a UTC timestamp stamp like "20260503-1234" for file names. */
function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return (
    `${now.getUTCFullYear()}${pad(now.getUTCMonth() + 1)}${pad(now.getUTCDate())}` +
    `-${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}`
  );
}

/** Kebab-case a free-text slug and cap at ~50 chars. */
function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 50);
}

const TS = timestamp();
const PREFIX = `${PROBE_NAME}-${PLATFORM}-${TS}`;

function writeFinding(opts: {
  slug: string;
  title: string;
  severity: 'crash' | 'spiral' | 'regression' | 'slowdown' | 'smell';
  file: string;
  line?: number;
  description: string;
  evidence?: string;
  proposedFix?: string;
}) {
  const slug = slugify(opts.slug);
  const findingId = `${PREFIX}-${slug}`;
  const path = join(OUTPUT_DIR, `${findingId}.md`);
  const frontmatter = [
    '---',
    `id: ${findingId}`,
    `work-item: ${PROBE_NAME}-${PLATFORM}`,
    `review: blackbox-probe`,
    `technique: 1`,
    `class: probe-smoke`,
    `severity: ${opts.severity}`,
    `file: ${opts.file}`,
    opts.line !== undefined ? `line: ${opts.line}` : '',
    `platforms-affected: [${PLATFORM}]`,
    `discovered-by: blackbox`,
    `discovered-at: ${new Date().toISOString()}`,
    `status: open`,
    '---',
  ]
    .filter(Boolean)
    .join('\n');

  const body = [
    `# ${opts.title}`,
    '',
    opts.description,
    opts.evidence ? `\n## Evidence\n\n${opts.evidence}` : '',
    opts.proposedFix ? `\n## Proposed fix\n\n${opts.proposedFix}` : '',
    '',
  ]
    .filter(Boolean)
    .join('\n');

  writeFileSync(path, `${frontmatter}\n\n${body}`);
  return path;
}

function writeDoneMarker(summary: {
  findingsEmitted: number;
  elapsedMs: number;
  metrics: Record<string, unknown>;
}) {
  const path = join(OUTPUT_DIR, `${PREFIX}-done.md`);
  const content = [
    '---',
    `id: ${PREFIX}-done`,
    `work-item: ${PROBE_NAME}-${PLATFORM}`,
    `kind: blackbox`,
    `platform: ${PLATFORM}`,
    `status: done`,
    `findings-emitted: ${summary.findingsEmitted}`,
    `elapsed-ms: ${summary.elapsedMs}`,
    `completed-at: ${new Date().toISOString()}`,
    `commit: ${process.env.KIRO_PROBE_COMMIT ?? ''}`,
    `ref: ${process.env.KIRO_PROBE_REF ?? ''}`,
    `build-mode: ${process.env.KIRO_PROBE_BUILD ?? ''}`,
    `bun-version: ${process.versions.bun}`,
    '---',
    '',
    `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
    '',
    `Emitted ${summary.findingsEmitted} finding(s) in ${summary.elapsedMs} ms.`,
    '',
    '## Metrics',
    '',
    '```json',
    JSON.stringify(summary.metrics, null, 2),
    '```',
    '',
  ].join('\n');
  writeFileSync(path, content);
  return path;
}

function writeMetrics(metrics: Record<string, unknown>) {
  const path = join(OUTPUT_DIR, `${PREFIX}-metrics.json`);
  writeFileSync(path, JSON.stringify(metrics, null, 2));
  return path;
}

function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();

  const metrics: Record<string, unknown> = {
    probe: PROBE_NAME,
    platform: PLATFORM,
    commit: process.env.KIRO_PROBE_COMMIT ?? '',
    ref: process.env.KIRO_PROBE_REF ?? '',
    buildMode: process.env.KIRO_PROBE_BUILD ?? '',
    bunVersionFromEnv: process.env.KIRO_PROBE_BUN_VERSION ?? '',
    bunVersion: process.versions.bun,
    nodeVersion: process.versions.node,
    rssMb: Math.round(process.memoryUsage().rss / 1024 / 1024),
    pid: process.pid,
    cwd: process.cwd(),
    arch: process.arch,
    isTTY: Boolean(process.stdout.isTTY),
    envCI: process.env.CI ?? '',
  };

  // Simulated "check": assert we can allocate a small buffer. Always passes
  // — this probe exists to exercise the file-emission plumbing, not to find
  // real bugs. A real probe would here detect a finding and call writeFinding.
  const buf = Buffer.alloc(1024 * 1024);
  buf.fill(0);

  const elapsedMs = Date.now() - started;
  const findingsEmitted = 0;

  const metricsPath = writeMetrics({
    ...metrics,
    elapsedMs,
    bufferSizeBytes: buf.byteLength,
  });
  const donePath = writeDoneMarker({
    findingsEmitted,
    elapsedMs,
    metrics: { ...metrics, elapsedMs },
  });

  console.log(
    `[${PROBE_NAME}] platform=${PLATFORM} rss=${metrics.rssMb}MB elapsed=${elapsedMs}ms — ok`
  );
  console.log(`  findings emitted: ${findingsEmitted}`);
  console.log(`  done marker:      ${donePath}`);
  console.log(`  metrics:          ${metricsPath}`);

  process.exit(findingsEmitted > 0 ? 1 : 0);
}

try {
  main();
} catch (err) {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const errorPath = join(OUTPUT_DIR, `${PREFIX}-error.log`);
  writeFileSync(
    errorPath,
    String(err instanceof Error ? (err.stack ?? err.message) : err)
  );
  console.error(`[${PROBE_NAME}] probe crashed on ${PLATFORM}:`);
  console.error(err);
  process.exit(2);
}

// Keep unused helper callable from other probes that want to emit findings.
export { writeFinding };
