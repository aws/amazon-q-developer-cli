#!/usr/bin/env bun
/**
 * heap-snapshot.ts — Review 03 technique 8 blackbox probe.
 *
 * Automated heap-snapshot diff for memory-leak detection. Launches the
 * TUI in test-mode with the built-in IPC socket, takes a baseline heap
 * snapshot via `Bun.generateHeapSnapshot()` (exposed over IPC by
 * `src/test-utils/TestModeProvider.tsx`), runs a 5-minute workload,
 * takes a second snapshot, and diffs the two by constructor name.
 *
 * This is strictly better than the `kill -USR2` path the runbook
 * describes — Bun's default snapshot is produced by the same API but
 * triggered reliably over a Unix socket, not by signal handler
 * race-prone behavior.
 *
 * ## Pass / fail
 *
 *   PASS if:
 *     - no single constructor grew by more than 100 % between snapshots
 *     - total heap delta < 50 MB
 *
 *   FAIL with the top 5 growing constructors listed otherwise.
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/heap-snapshot.ts
 *
 * Environment:
 *   HEAP_WORKLOAD_MS     workload duration between snapshots
 *                        (default: 300000 = 5 min; set to 30000 for CI)
 *   PROBE_OUTPUT_DIR     where to write findings/metrics/snapshots
 */

import * as net from 'node:net';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { PtyManager } from '../../src/test-utils/shared/pty-manager';
import { TuiIpcConnection } from '../../src/test-utils/shared/tui-ipc-connection';

const PROBE_NAME = 'heap-snapshot';
const PLATFORM =
  process.env.KIRO_PROBE_PLATFORM ??
  (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';

const DEFAULT_WORKLOAD_MS = process.env.CI === '1' ? 30_000 : 300_000;
const WORKLOAD_MS = parseInt(
  process.env.HEAP_WORKLOAD_MS ?? String(DEFAULT_WORKLOAD_MS),
  10
);

// Budgets
const PER_CONSTRUCTOR_GROWTH_PCT_BUDGET = 100; // max 100 % growth
const TOTAL_HEAP_GROWTH_MB_BUDGET = 50;
const MIN_BYTES_TO_TRACK = 64 * 1024; // ignore constructors < 64 KB baseline

function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getUTCFullYear()}${pad(now.getUTCMonth() + 1)}${pad(now.getUTCDate())}-${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}`;
}

function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 50);
}

const TS = timestamp();
const PREFIX = `${PROBE_NAME}-${PLATFORM}-${TS}`;

interface ConstructorStat {
  name: string;
  count: number;
  selfSizeBytes: number;
}

/**
 * Parse a V8/Bun heap snapshot (JSON). Returns per-constructor aggregated
 * count and self_size. The format is documented at
 * https://v8.dev/blog/chrome-heap-profiler (node_fields + flat nodes array).
 */
function parseSnapshot(path: string): Map<string, ConstructorStat> {
  const raw = readFileSync(path, 'utf-8');
  const json = JSON.parse(raw);
  const meta = json.snapshot?.meta;
  const nodeFields: string[] = meta?.node_fields ?? [];
  const nodeTypes: string[] = (meta?.node_types?.[0] as string[]) ?? [];
  const nodes: number[] = json.nodes ?? [];
  const strings: string[] = json.strings ?? [];

  if (nodeFields.length === 0 || nodes.length === 0) {
    throw new Error(`Heap snapshot at ${path} has no parseable nodes`);
  }

  const typeIdx = nodeFields.indexOf('type');
  const nameIdx = nodeFields.indexOf('name');
  const selfSizeIdx = nodeFields.indexOf('self_size');
  if (typeIdx < 0 || nameIdx < 0 || selfSizeIdx < 0) {
    throw new Error(
      `Heap snapshot at ${path} missing required node_fields (type/name/self_size)`
    );
  }
  const fieldCount = nodeFields.length;

  const stats = new Map<string, ConstructorStat>();
  for (let i = 0; i < nodes.length; i += fieldCount) {
    const typeCode = nodes[i + typeIdx];
    const nameCode = nodes[i + nameIdx];
    const selfSize = nodes[i + selfSizeIdx];
    if (typeCode === undefined || nameCode === undefined || selfSize === undefined) {
      continue;
    }
    const typeName = nodeTypes[typeCode] ?? 'unknown';

    // For object/closure/code nodes, the name is the constructor. For
    // strings, the name is the string preview — bucket those under a
    // single "<string>" key. Same for other synthetic types.
    let bucket: string;
    if (typeName === 'object' || typeName === 'closure') {
      bucket = strings[nameCode] ?? '<anon>';
    } else if (
      typeName === 'string' ||
      typeName === 'concatenated string' ||
      typeName === 'sliced string'
    ) {
      bucket = '<string>';
    } else {
      bucket = `<${typeName}>`;
    }

    let s = stats.get(bucket);
    if (!s) {
      s = { name: bucket, count: 0, selfSizeBytes: 0 };
      stats.set(bucket, s);
    }
    s.count++;
    s.selfSizeBytes += selfSize;
  }
  return stats;
}

interface ConstructorDelta {
  name: string;
  baselineCount: number;
  postCount: number;
  countDelta: number;
  baselineBytes: number;
  postBytes: number;
  bytesDelta: number;
  growthPct: number;
}

function diff(
  baseline: Map<string, ConstructorStat>,
  post: Map<string, ConstructorStat>
): ConstructorDelta[] {
  const names = new Set([...baseline.keys(), ...post.keys()]);
  const out: ConstructorDelta[] = [];
  for (const name of names) {
    const b = baseline.get(name);
    const p = post.get(name);
    const baselineBytes = b?.selfSizeBytes ?? 0;
    const postBytes = p?.selfSizeBytes ?? 0;
    // Skip noise: constructors that are tiny in both snapshots.
    if (baselineBytes < MIN_BYTES_TO_TRACK && postBytes < MIN_BYTES_TO_TRACK) {
      continue;
    }
    const bytesDelta = postBytes - baselineBytes;
    const growthPct =
      baselineBytes > 0
        ? (bytesDelta / baselineBytes) * 100
        : postBytes > 0
          ? Infinity
          : 0;
    out.push({
      name,
      baselineCount: b?.count ?? 0,
      postCount: p?.count ?? 0,
      countDelta: (p?.count ?? 0) - (b?.count ?? 0),
      baselineBytes,
      postBytes,
      bytesDelta,
      growthPct,
    });
  }
  // Sort by bytesDelta descending (most grown first)
  out.sort((a, b) => b.bytesDelta - a.bytesDelta);
  return out;
}

function writeFinding(opts: {
  slug: string;
  title: string;
  severity: 'crash' | 'spiral' | 'regression' | 'slowdown' | 'smell';
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
    `work-item: 03-session-lifetime-blackbox`,
    `review: 03-unbounded-growth`,
    `technique: 8`,
    `class: heap-snapshot-diff`,
    `severity: ${opts.severity}`,
    `file: packages/tui/src/stores/app-store.ts`,
    `platforms-affected: [${PLATFORM}]`,
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

  writeFileSync(path, `${frontmatter}\n\n${body}\n`);
  return path;
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();

  // Set up test-mode IPC: unix socket server + TUI with env vars
  const socketPath = join(
    tmpdir(),
    `kiro-probe-${PROBE_NAME}-${process.pid}.sock`
  );
  const tuiIndex = join(process.cwd(), 'packages/tui/src/index.tsx');

  let connection: TuiIpcConnection | undefined;
  const server = net.createServer((socket) => {
    connection = new TuiIpcConnection(socket);
  });
  await new Promise<void>((resolve, reject) => {
    server.listen(socketPath, (err?: Error) => (err ? reject(err) : resolve()));
  });

  const ptyMgr = new PtyManager({
    width: 120,
    height: 40,
    env: {
      KIRO_TEST_MODE: 'true',
      KIRO_MOCK_ACP: 'true',
      KIRO_TEST_TUI_IPC_SOCKET_PATH: socketPath,
      KIRO_DISABLE_TELEMETRY: '1',
      TERM: 'xterm-color',
    },
  });
  ptyMgr.spawn('bun', ['run', tuiIndex]);

  // Wait for IPC connection (TUI finished initial render and connected)
  const connectTimeoutMs = 15_000;
  const connectStart = Date.now();
  while (!connection && Date.now() - connectStart < connectTimeoutMs) {
    await Bun.sleep(100);
  }
  if (!connection) {
    ptyMgr.kill();
    server.close();
    throw new Error(
      `IPC connection from TUI did not arrive within ${connectTimeoutMs} ms. ` +
        `Check that ${tuiIndex} exists and TestModeProvider is wired in.`
    );
  }

  // Force a GC, then take baseline snapshot
  const baselinePath = join(OUTPUT_DIR, `${PREFIX}-baseline.heapsnapshot`);
  await connection.sendCommand({ kind: 'FORCE_GC' });
  await Bun.sleep(500);
  await connection.sendCommand({ kind: 'HEAP_SNAPSHOT', filename: baselinePath });

  const baselineMemResp = await connection.sendCommand({ kind: 'MEMORY_USAGE' });
  const baselineMem =
    baselineMemResp.data.kind === 'MEMORY_USAGE' ? baselineMemResp.data.data : null;

  // Run workload: hammer keystrokes for WORKLOAD_MS
  const workloadStart = Date.now();
  let keystrokesSent = 0;
  while (Date.now() - workloadStart < WORKLOAD_MS) {
    try {
      await ptyMgr.sendKeys(`probe ${keystrokesSent}\n`);
      keystrokesSent++;
    } catch {
      /* ignore */
    }
    // 50-100 ms spacing keeps the event loop responsive without overwhelming it
    await Bun.sleep(80);
  }

  // Force GC, take second snapshot
  await connection.sendCommand({ kind: 'FORCE_GC' });
  await Bun.sleep(500);
  const postPath = join(OUTPUT_DIR, `${PREFIX}-post.heapsnapshot`);
  await connection.sendCommand({ kind: 'HEAP_SNAPSHOT', filename: postPath });

  const postMemResp = await connection.sendCommand({ kind: 'MEMORY_USAGE' });
  const postMem =
    postMemResp.data.kind === 'MEMORY_USAGE' ? postMemResp.data.data : null;

  // Clean up TUI
  connection.close();
  ptyMgr.kill();
  server.close();

  // Analyze snapshots
  const baselineStats = parseSnapshot(baselinePath);
  const postStats = parseSnapshot(postPath);
  const deltas = diff(baselineStats, postStats);

  const totalBaselineBytes = [...baselineStats.values()].reduce(
    (a, s) => a + s.selfSizeBytes,
    0
  );
  const totalPostBytes = [...postStats.values()].reduce(
    (a, s) => a + s.selfSizeBytes,
    0
  );
  const totalDeltaMb = (totalPostBytes - totalBaselineBytes) / 1024 / 1024;

  // Find constructors that violated the per-constructor growth budget
  const violators = deltas.filter(
    (d) =>
      d.growthPct > PER_CONSTRUCTOR_GROWTH_PCT_BUDGET &&
      d.bytesDelta > MIN_BYTES_TO_TRACK
  );

  // Top 5 growing for evidence regardless of pass/fail
  const top5 = deltas.slice(0, 5);

  const findings: Array<{ slug: string; title: string; severity: 'spiral' }> = [];
  if (totalDeltaMb > TOTAL_HEAP_GROWTH_MB_BUDGET) {
    findings.push({
      slug: 'total-heap-growth',
      title: `Total heap grew ${totalDeltaMb.toFixed(1)} MB (budget: ${TOTAL_HEAP_GROWTH_MB_BUDGET} MB)`,
      severity: 'spiral',
    });
  }
  if (violators.length > 0) {
    findings.push({
      slug: 'per-constructor-growth',
      title: `${violators.length} constructor(s) grew by more than ${PER_CONSTRUCTOR_GROWTH_PCT_BUDGET} %`,
      severity: 'spiral',
    });
  }

  const top5Evidence = top5
    .map(
      (d, i) =>
        `${i + 1}. ${d.name}: ` +
        `${(d.baselineBytes / 1024).toFixed(1)} KB → ${(d.postBytes / 1024).toFixed(1)} KB ` +
        `(Δ ${(d.bytesDelta / 1024).toFixed(1)} KB, ${d.growthPct === Infinity ? '∞' : d.growthPct.toFixed(0)} %, ` +
        `count ${d.baselineCount} → ${d.postCount})`
    )
    .join('\n');

  for (const f of findings) {
    writeFinding({
      slug: f.slug,
      title: f.title,
      severity: f.severity,
      description: f.title,
      evidence: [
        `Workload duration: ${(WORKLOAD_MS / 1000).toFixed(0)} s`,
        `Keystrokes sent: ${keystrokesSent}`,
        `Total heap baseline: ${(totalBaselineBytes / 1024 / 1024).toFixed(1)} MB`,
        `Total heap post-workload: ${(totalPostBytes / 1024 / 1024).toFixed(1)} MB`,
        `Total delta: ${totalDeltaMb.toFixed(1)} MB`,
        baselineMem
          ? `process.memoryUsage baseline RSS: ${(baselineMem.rss / 1024 / 1024).toFixed(1)} MB`
          : '',
        postMem
          ? `process.memoryUsage post RSS: ${(postMem.rss / 1024 / 1024).toFixed(1)} MB`
          : '',
        '',
        'Top 5 growing constructors (by self_size delta):',
        top5Evidence,
        '',
        `Snapshots retained for inspection:`,
        `  baseline: ${baselinePath}`,
        `  post:     ${postPath}`,
      ]
        .filter(Boolean)
        .join('\n'),
      proposedFix:
        'Inspect the top growing constructors in Chrome DevTools Memory view ' +
        '(load both .heapsnapshot files, use the Comparison view). Focus on ' +
        'closures/objects retained by long-lived stores (app-store, ' +
        'session-conversations) and EventEmitter listener maps.',
    });
  }

  const elapsedMs = Date.now() - started;
  const metrics = {
    probe: PROBE_NAME,
    platform: PLATFORM,
    elapsedMs,
    workloadMs: WORKLOAD_MS,
    keystrokesSent,
    baselineHeapBytes: totalBaselineBytes,
    postHeapBytes: totalPostBytes,
    totalDeltaMb: +totalDeltaMb.toFixed(2),
    baselineRssBytes: baselineMem?.rss ?? null,
    postRssBytes: postMem?.rss ?? null,
    top5: top5.map((d) => ({
      name: d.name,
      baselineBytes: d.baselineBytes,
      postBytes: d.postBytes,
      bytesDelta: d.bytesDelta,
      growthPct: d.growthPct === Infinity ? null : +d.growthPct.toFixed(2),
      baselineCount: d.baselineCount,
      postCount: d.postCount,
    })),
    violatorCount: violators.length,
    budgets: {
      perConstructorGrowthPct: PER_CONSTRUCTOR_GROWTH_PCT_BUDGET,
      totalHeapGrowthMb: TOTAL_HEAP_GROWTH_MB_BUDGET,
    },
    pass: findings.length === 0,
  };
  writeFileSync(
    join(OUTPUT_DIR, `${PREFIX}-metrics.json`),
    JSON.stringify(metrics, null, 2)
  );

  writeFileSync(
    join(OUTPUT_DIR, `${PREFIX}-done.md`),
    [
      '---',
      `id: ${PREFIX}-done`,
      `work-item: 03-session-lifetime-blackbox`,
      `kind: blackbox`,
      `platform: ${PLATFORM}`,
      `status: done`,
      `findings-emitted: ${findings.length}`,
      `elapsed-ms: ${elapsedMs}`,
      `completed-at: ${new Date().toISOString()}`,
      '---',
      '',
      `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
      '',
      `Emitted ${findings.length} finding(s) after ${(WORKLOAD_MS / 1000).toFixed(0)}s workload in ${elapsedMs} ms.`,
      '',
    ].join('\n')
  );

  const pass = findings.length === 0;
  console.log(`\n[${PROBE_NAME}] ${pass ? '✅ PASS' : '❌ FAIL'} on ${PLATFORM}`);
  console.log(`  workload:       ${(WORKLOAD_MS / 1000).toFixed(0)} s`);
  console.log(`  keystrokes:     ${keystrokesSent}`);
  console.log(
    `  heap baseline:  ${(totalBaselineBytes / 1024 / 1024).toFixed(1)} MB`
  );
  console.log(`  heap post:      ${(totalPostBytes / 1024 / 1024).toFixed(1)} MB`);
  console.log(`  delta:          ${totalDeltaMb.toFixed(1)} MB`);
  console.log(`  violators:      ${violators.length}`);
  console.log(`  top 5 growing:`);
  for (const d of top5) {
    console.log(
      `    ${d.name}: Δ ${(d.bytesDelta / 1024).toFixed(1)} KB (${d.growthPct === Infinity ? '∞' : d.growthPct.toFixed(0)} %)`
    );
  }
  if (!pass) for (const f of findings) console.log(`  • ${f.title}`);

  process.exit(pass ? 0 : 1);
}

try {
  await main();
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
