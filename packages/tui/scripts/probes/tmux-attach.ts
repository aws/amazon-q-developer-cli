#!/usr/bin/env bun
/**
 * tmux-attach.ts — blackbox probe for tmux attach/detach FD & memory leaks.
 *
 * Verifies that repeated tmux attach/detach cycles do not leak file descriptors
 * or cause unbounded memory growth in the chat_cli process.
 *
 * Pass criteria:
 *   - FD count growth < 10 over baseline
 *   - Memory growth < 30 MB
 *   - Process still alive after all cycles
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = crash
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/tmux-attach.ts
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { execSync, spawnSync } from 'node:child_process';

const PROBE_NAME = 'tmux-attach';
const PLATFORM = process.env.KIRO_PROBE_PLATFORM ?? (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
const SESSION = 'kiro-probe-tmux-attach';
const CYCLES = 20;
const INTERVAL_MS = 500;

function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getUTCFullYear()}${pad(now.getUTCMonth() + 1)}${pad(now.getUTCDate())}-${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}`;
}

function slugify(text: string): string {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 50);
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
    `class: probe-tmux-attach`,
    `severity: ${opts.severity}`,
    `file: ${opts.file}`,
    opts.line !== undefined ? `line: ${opts.line}` : '',
    `platforms-affected: [${PLATFORM}]`,
    `discovered-by: blackbox`,
    `discovered-at: ${new Date().toISOString()}`,
    `status: open`,
    '---',
  ].filter(Boolean).join('\n');

  const body = [
    `# ${opts.title}`,
    '',
    opts.description,
    opts.evidence ? `\n## Evidence\n\n${opts.evidence}` : '',
    opts.proposedFix ? `\n## Proposed fix\n\n${opts.proposedFix}` : '',
  ].filter(Boolean).join('\n');

  writeFileSync(path, `${frontmatter}\n\n${body}\n`);
  return path;
}

function writeMetrics(metrics: Record<string, unknown>) {
  const path = join(OUTPUT_DIR, `${PREFIX}-metrics.json`);
  writeFileSync(path, JSON.stringify(metrics, null, 2));
  return path;
}

function writeDoneMarker(summary: { findingsEmitted: number; elapsedMs: number; metrics: Record<string, unknown> }) {
  const path = join(OUTPUT_DIR, `${PREFIX}-done.md`);
  writeFileSync(path, [
    '---',
    `id: ${PREFIX}-done`,
    `work-item: ${PROBE_NAME}-${PLATFORM}`,
    `kind: blackbox`,
    `platform: ${PLATFORM}`,
    `status: done`,
    `findings-emitted: ${summary.findingsEmitted}`,
    `elapsed-ms: ${summary.elapsedMs}`,
    `completed-at: ${new Date().toISOString()}`,
    '---',
    '',
    `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
    '',
    `Emitted ${summary.findingsEmitted} finding(s) in ${summary.elapsedMs} ms.`,
    '',
    '```json',
    JSON.stringify(summary.metrics, null, 2),
    '```',
    '',
  ].join('\n'));
  return path;
}

function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

function exec(cmd: string): string {
  return execSync(cmd, { encoding: 'utf-8', timeout: 10_000 }).trim();
}

function hasTmux(): boolean {
  return spawnSync('which', ['tmux']).status === 0;
}

function getFdCount(pid: number): number {
  if (process.platform === 'linux') {
    try {
      return parseInt(exec(`ls /proc/${pid}/fd | wc -l`), 10);
    } catch { /* fall through to lsof */ }
  }
  // macOS or Linux fallback
  const output = exec(`lsof -p ${pid} 2>/dev/null | wc -l`);
  return parseInt(output, 10);
}

function getRssKb(pid: number): number {
  const output = exec(`ps -o rss= -p ${pid}`);
  return parseInt(output, 10);
}

function isProcessAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function getChatCliPid(): number | null {
  try {
    // Find the chat_cli process running inside our tmux session
    const pane_pid = exec(`tmux display-message -t ${SESSION} -p '#{pane_pid}'`);
    if (pane_pid) return parseInt(pane_pid, 10);
  } catch { /* ignore */ }
  try {
    const output = exec(`pgrep -f "chat_cli.*chat.*--tui" | head -1`);
    if (output) return parseInt(output, 10);
  } catch { /* ignore */ }
  return null;
}

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();

  // Pre-flight: check tmux
  if (!hasTmux()) {
    console.log(`[${PROBE_NAME}] tmux not installed — skipping probe.`);
    process.exit(0);
  }

  // Kill any leftover session
  spawnSync('tmux', ['kill-session', '-t', SESSION], { stdio: 'ignore' });

  // Determine the TUI JS path and build command
  const tuiJsPath = process.env.KIRO_TEST_TUI_JS_PATH ?? `${process.cwd()}/packages/tui/dist/tui.js`;
  const chatCmd = `KIRO_TEST_TUI_JS_PATH=${tuiJsPath} KIRO_TEST_MODE=1 KIRO_DISABLE_TELEMETRY=1 cargo run -p chat_cli -- chat --tui`;

  // Start tmux session with chat_cli
  execSync(`tmux new-session -d -s ${SESSION} -x 120 -y 40 '${chatCmd}'`, { stdio: 'ignore' });

  // Wait for process to start
  await sleep(5000);

  const pid = getChatCliPid();
  if (!pid || !isProcessAlive(pid)) {
    console.error(`[${PROBE_NAME}] Failed to find chat_cli process.`);
    spawnSync('tmux', ['kill-session', '-t', SESSION], { stdio: 'ignore' });
    process.exit(2);
  }

  // Baseline measurements
  const baselineFd = getFdCount(pid);
  const baselineRssKb = getRssKb(pid);

  console.log(`[${PROBE_NAME}] pid=${pid} baseline: fd=${baselineFd} rss=${(baselineRssKb / 1024).toFixed(1)}MB`);

  // Perform attach/detach cycles
  for (let i = 1; i <= CYCLES; i++) {
    spawnSync('tmux', ['detach-client', '-t', SESSION], { stdio: 'ignore' });
    await sleep(INTERVAL_MS / 2);
    // Attach in detached mode (non-blocking)
    spawnSync('tmux', ['attach-session', '-t', SESSION, '-d'], {
      stdio: 'ignore',
      timeout: 2000,
    });
    await sleep(INTERVAL_MS / 2);

    if (!isProcessAlive(pid)) {
      console.error(`[${PROBE_NAME}] Process crashed at cycle ${i}`);
      break;
    }
  }

  // Wait for settling
  await sleep(2000);

  // Final measurements
  const alive = isProcessAlive(pid);
  const finalFd = alive ? getFdCount(pid) : -1;
  const finalRssKb = alive ? getRssKb(pid) : -1;

  // Cleanup
  spawnSync('tmux', ['kill-session', '-t', SESSION], { stdio: 'ignore' });

  const fdGrowth = finalFd - baselineFd;
  const rssGrowthMb = (finalRssKb - baselineRssKb) / 1024;
  const elapsedMs = Date.now() - started;

  const metrics = {
    probe: PROBE_NAME,
    platform: PLATFORM,
    pid,
    cycles: CYCLES,
    intervalMs: INTERVAL_MS,
    baselineFd,
    finalFd,
    fdGrowth,
    baselineRssMb: +(baselineRssKb / 1024).toFixed(1),
    finalRssMb: +(finalRssKb / 1024).toFixed(1),
    rssGrowthMb: +rssGrowthMb.toFixed(1),
    processAlive: alive,
    elapsedMs,
  };

  let findingsEmitted = 0;

  // Check: process crashed
  if (!alive) {
    writeFinding({
      slug: 'process-crash',
      title: 'chat_cli crashed during tmux attach/detach cycles',
      severity: 'crash',
      file: 'packages/tui/src/index.tsx',
      description: `The chat_cli process (pid ${pid}) died during ${CYCLES} attach/detach cycles.`,
      evidence: JSON.stringify(metrics, null, 2),
    });
    findingsEmitted++;
  }

  // Check: FD leak
  if (alive && fdGrowth >= 10) {
    writeFinding({
      slug: 'fd-leak',
      title: `FD count grew by ${fdGrowth} during attach/detach cycles`,
      severity: 'spiral',
      file: 'packages/tui/src/index.tsx',
      description: `FD count grew from ${baselineFd} to ${finalFd} (+${fdGrowth}) over ${CYCLES} attach/detach cycles. Budget is <10.`,
      evidence: JSON.stringify(metrics, null, 2),
      proposedFix: 'Ensure PTY file descriptors and event listeners are cleaned up on detach.',
    });
    findingsEmitted++;
  }

  // Check: memory leak
  if (alive && rssGrowthMb >= 30) {
    writeFinding({
      slug: 'memory-growth',
      title: `RSS grew ${rssGrowthMb.toFixed(1)}MB during attach/detach cycles`,
      severity: 'spiral',
      file: 'packages/tui/src/index.tsx',
      description: `RSS grew from ${(baselineRssKb / 1024).toFixed(1)}MB to ${(finalRssKb / 1024).toFixed(1)}MB (+${rssGrowthMb.toFixed(1)}MB) over ${CYCLES} cycles. Budget is <30MB.`,
      evidence: JSON.stringify(metrics, null, 2),
      proposedFix: 'Check for retained render buffers or event listener accumulation on terminal resize/reattach.',
    });
    findingsEmitted++;
  }

  writeMetrics(metrics);
  writeDoneMarker({ findingsEmitted, elapsedMs, metrics });

  console.log(`[${PROBE_NAME}] platform=${PLATFORM} cycles=${CYCLES} fd_growth=${fdGrowth} rss_growth=${rssGrowthMb.toFixed(1)}MB alive=${alive} elapsed=${elapsedMs}ms`);
  console.log(`  result: ${findingsEmitted === 0 ? 'PASS' : 'FAIL'} (${findingsEmitted} finding(s))`);

  process.exit(findingsEmitted > 0 ? 1 : 0);
}

try {
  await main();
} catch (err) {
  // Cleanup on crash
  spawnSync('tmux', ['kill-session', '-t', SESSION], { stdio: 'ignore' });
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const errorPath = join(OUTPUT_DIR, `${PREFIX}-error.log`);
  writeFileSync(errorPath, String(err instanceof Error ? (err.stack ?? err.message) : err));
  console.error(`[${PROBE_NAME}] probe crashed on ${PLATFORM}:`);
  console.error(err);
  process.exit(2);
}
