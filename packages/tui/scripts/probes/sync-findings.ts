#!/usr/bin/env bun
/**
 * sync-findings.ts — download blackbox-probe workflow artifacts and copy
 * finding files into the playbook's findings directory.
 *
 * This helper is intentionally dumb. It moves files from an artifact into
 * the findings directory and stops. It does not commit, does not
 * interpret, does not merge. That keeps it safe to run from any context
 * (CI, subagent, human, Ralph).
 *
 * Usage:
 *   bun run packages/tui/scripts/probes/sync-findings.ts --latest
 *   bun run packages/tui/scripts/probes/sync-findings.ts --run-id 1234567890
 *   bun run packages/tui/scripts/probes/sync-findings.ts --run-id 1234567890 --repo jsamuel1/kiro-cli
 *
 * Dry-run (print what would happen, touch nothing):
 *   bun run packages/tui/scripts/probes/sync-findings.ts --latest --dry-run
 *
 * Test against a local directory of pre-downloaded artifacts (no `gh` call):
 *   bun run packages/tui/scripts/probes/sync-findings.ts --from-dir /tmp/my-artifacts
 *
 * Requires `gh` CLI to be installed and authenticated unless --from-dir is used.
 *
 * Exit codes:
 *   0  success
 *   1  bad arguments / workflow not found / no artifacts
 *   2  gh not installed or not authenticated
 */

import { spawnSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  rmSync,
  statSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';

const WORKFLOW = 'blackbox-probe.yml';
const FINDINGS_DIR = resolve(
  __dirname,
  '../../../../docs/review-playbook/runner/findings'
);

interface Args {
  runId?: string;
  latest: boolean;
  repo?: string;
  fromDir?: string;
  dryRun: boolean;
  findingsDir: string;
}

function parseArgs(argv: string[]): Args {
  const args: Args = {
    latest: false,
    dryRun: false,
    findingsDir: FINDINGS_DIR,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    switch (a) {
      case '--latest':
        args.latest = true;
        break;
      case '--run-id':
        args.runId = argv[++i];
        break;
      case '--repo':
        args.repo = argv[++i];
        break;
      case '--from-dir':
        args.fromDir = argv[++i];
        break;
      case '--dry-run':
        args.dryRun = true;
        break;
      case '--findings-dir':
        args.findingsDir = resolve(argv[++i] ?? '.');
        break;
      case '--help':
      case '-h':
        printHelpAndExit(0);
        break; // unreachable; satisfies no-fallthrough
      default:
        console.error(`Unknown argument: ${a}`);
        printHelpAndExit(1);
    }
  }
  if (!args.latest && !args.runId && !args.fromDir) {
    console.error(
      'Provide one of --latest, --run-id <id>, or --from-dir <path>'
    );
    printHelpAndExit(1);
  }
  return args;
}

function printHelpAndExit(code: number): never {
  console.error(`\nsync-findings.ts — sync probe artifacts into findings/\n`);
  console.error(`Usage:`);
  console.error(
    `  sync-findings.ts (--latest | --run-id <id> | --from-dir <path>) [options]`
  );
  console.error(``);
  console.error(`Options:`);
  console.error(`  --latest                 Most recent run of ${WORKFLOW}`);
  console.error(`  --run-id <id>            Specific workflow run id`);
  console.error(
    `  --repo <owner/repo>      GitHub repo (default: current repo from gh)`
  );
  console.error(
    `  --from-dir <path>        Local directory of artifact subdirs (skip gh)`
  );
  console.error(`  --findings-dir <path>    Override destination dir`);
  console.error(
    `  --dry-run                Print what would happen; touch nothing`
  );
  console.error(`  -h, --help               Show this help`);
  process.exit(code);
}

/** Check that `gh` is installed and the user is logged in. */
function ensureGhAvailable() {
  const which = spawnSync('gh', ['--version'], { encoding: 'utf8' });
  if (which.status !== 0) {
    console.error('gh CLI not found. Install from https://cli.github.com/');
    process.exit(2);
  }
  const auth = spawnSync('gh', ['auth', 'status'], { encoding: 'utf8' });
  if (auth.status !== 0) {
    console.error('gh CLI not authenticated. Run: gh auth login');
    process.exit(2);
  }
}

/** Resolve `--latest` to a concrete run id. */
function resolveLatestRunId(repo: string | undefined): string {
  const args = [
    'run',
    'list',
    '--workflow',
    WORKFLOW,
    '--limit',
    '1',
    '--json',
    'databaseId,status,conclusion,createdAt',
  ];
  if (repo) args.push('--repo', repo);
  const r = spawnSync('gh', args, { encoding: 'utf8' });
  if (r.status !== 0) {
    console.error(`gh run list failed: ${r.stderr.trim()}`);
    process.exit(1);
  }
  const runs = JSON.parse(r.stdout) as Array<{
    databaseId: number;
    status: string;
    conclusion: string | null;
    createdAt: string;
  }>;
  if (runs.length === 0) {
    console.error(
      `No runs found for workflow ${WORKFLOW}${repo ? ` in ${repo}` : ''}.`
    );
    process.exit(1);
  }
  const run = runs[0];
  if (!run) {
    console.error('No runs found');
    process.exit(1);
  }
  console.log(
    `Latest run: ${run.databaseId} (${run.status}/${run.conclusion ?? 'pending'}) at ${run.createdAt}`
  );
  return String(run.databaseId);
}

/** Download every artifact from the given run into `targetDir`. */
function ghDownload(
  runId: string,
  repo: string | undefined,
  targetDir: string
) {
  const args = ['run', 'download', runId, '--dir', targetDir];
  if (repo) args.push('--repo', repo);
  const r = spawnSync('gh', args, { encoding: 'utf8', stdio: 'inherit' });
  if (r.status !== 0) {
    console.error(`gh run download failed with status ${r.status}`);
    process.exit(1);
  }
}

/**
 * Walk every subdirectory of `sourceDir` (one per artifact) and copy finding
 * files, markers, and metrics into `findingsDir`. Returns a manifest of what
 * was copied so the summary is accurate.
 */
function collectArtifactFiles(
  sourceDir: string
): { source: string; basename: string }[] {
  const out: { source: string; basename: string }[] = [];
  if (!existsSync(sourceDir)) return out;

  for (const entry of readdirSync(sourceDir)) {
    const full = join(sourceDir, entry);
    const s = statSync(full);
    if (s.isDirectory()) {
      // Artifact subdirectory. Walk it one level deep — probes write flat.
      for (const inner of readdirSync(full)) {
        const innerFull = join(full, inner);
        if (statSync(innerFull).isFile() && isArtifactFile(inner)) {
          out.push({ source: innerFull, basename: inner });
        }
      }
    } else if (s.isFile() && isArtifactFile(entry)) {
      // Top-level file (rare, but gh sometimes flattens single-artifact runs).
      out.push({ source: full, basename: entry });
    }
  }
  return out;
}

function isArtifactFile(name: string): boolean {
  // Finding file, marker, metrics, error log. Skip `.log` and `.meta` which
  // are workflow-plumbing records, not findings.
  return (
    name.endsWith('.md') ||
    name.endsWith('.metrics.json') ||
    name.endsWith('-metrics.json') ||
    name.endsWith('.error.log')
  );
}

function classify(
  name: string
):
  | 'finding'
  | 'done'
  | 'continuation'
  | 'runbook'
  | 'metrics'
  | 'error'
  | 'other' {
  if (name.endsWith('-metrics.json')) return 'metrics';
  if (name.endsWith('.metrics.json')) return 'metrics';
  if (name.endsWith('.error.log')) return 'error';
  if (name.endsWith('-done.md')) return 'done';
  if (name.endsWith('-continuation.md')) return 'continuation';
  if (name.endsWith('-runbook.md')) return 'runbook';
  if (name.endsWith('.md')) return 'finding';
  return 'other';
}

function copyIntoFindings(
  files: { source: string; basename: string }[],
  findingsDir: string,
  dryRun: boolean
) {
  if (!dryRun) mkdirSync(findingsDir, { recursive: true });

  const counts = {
    finding: 0,
    done: 0,
    continuation: 0,
    runbook: 0,
    metrics: 0,
    error: 0,
    other: 0,
    skipped: 0,
  };
  for (const f of files) {
    const dest = join(findingsDir, f.basename);
    const kind = classify(f.basename);
    if (existsSync(dest)) {
      console.log(`  = ${f.basename} (already present, skipping)`);
      counts.skipped++;
      continue;
    }
    if (dryRun) {
      console.log(`  + ${f.basename} (${kind}) — would copy`);
    } else {
      copyFileSync(f.source, dest);
      console.log(`  + ${f.basename} (${kind})`);
    }
    counts[kind]++;
  }
  return counts;
}

function printSummary(
  counts: Record<string, number>,
  target: string,
  dryRun: boolean
) {
  const total = Object.values(counts).reduce((a, b) => a + b, 0);
  console.log('');
  console.log(`${dryRun ? '[dry-run] ' : ''}Sync complete.`);
  console.log(`  Target:         ${target}`);
  console.log(`  Finding files:  ${counts.finding ?? 0}`);
  console.log(`  Done markers:   ${counts.done ?? 0}`);
  console.log(`  Continuations:  ${counts.continuation ?? 0}`);
  console.log(`  Runbooks:       ${counts.runbook ?? 0}`);
  console.log(`  Metrics:        ${counts.metrics ?? 0}`);
  console.log(`  Error logs:     ${counts.error ?? 0}`);
  console.log(`  Other:          ${counts.other ?? 0}`);
  console.log(`  Already present: ${counts.skipped ?? 0}`);
  console.log(`  Total seen:     ${total}`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));

  let sourceDir: string;
  let cleanupDir: string | undefined;

  if (args.fromDir) {
    sourceDir = resolve(args.fromDir);
    if (!existsSync(sourceDir)) {
      console.error(`--from-dir path does not exist: ${sourceDir}`);
      process.exit(1);
    }
    console.log(`Reading artifacts from ${sourceDir}`);
  } else {
    ensureGhAvailable();
    const runId = args.runId ?? resolveLatestRunId(args.repo);
    const tempDir = mkdtempSync(join(tmpdir(), 'kiro-probe-sync-'));
    cleanupDir = tempDir;
    console.log(`Downloading artifacts for run ${runId} to ${tempDir}`);
    ghDownload(runId, args.repo, tempDir);
    sourceDir = tempDir;
  }

  console.log(`\nCopying into ${args.findingsDir}:`);
  const files = collectArtifactFiles(sourceDir);
  if (files.length === 0) {
    console.error(`No artifact files found under ${sourceDir}. Nothing to do.`);
    if (cleanupDir) rmSync(cleanupDir, { recursive: true, force: true });
    process.exit(1);
  }

  const counts = copyIntoFindings(files, args.findingsDir, args.dryRun);
  printSummary(counts, args.findingsDir, args.dryRun);

  if (cleanupDir && !args.dryRun) {
    rmSync(cleanupDir, { recursive: true, force: true });
  } else if (cleanupDir) {
    console.log(`\nDry-run: left temp dir at ${cleanupDir} for inspection`);
  }

  console.log('');
  console.log('Next step: review findings/ and commit the ones worth keeping.');
  console.log('  git add docs/review-playbook/runner/findings/');
  console.log('  git status docs/review-playbook/runner/findings/');
}

main().catch((err) => {
  console.error(`sync-findings failed:`);
  console.error(err);
  process.exit(2);
});

// Ignore unused suppression.
void basename;
