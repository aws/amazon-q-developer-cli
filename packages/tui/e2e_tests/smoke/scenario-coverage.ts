#!/usr/bin/env bun
/**
 * Scenario coverage: merge per-scenario lcov output and check drift.
 *
 * The scenario runner (run-smoke.ts --coverage) writes one lcov.info per
 * scenario under <coverage-dir>/<scenario-id>/. This tool merges them into a
 * single per-file line-coverage summary and compares it against the committed
 * floor (scenarios.coverage.json). Coverage below the floor is drift: source
 * the suite used to exercise is no longer exercised.
 *
 * check exits 1 on drift so a CI step can surface it; the lane that runs it
 * decides whether that blocks (today: advisory only). A missing floor is not
 * drift — it reports bootstrap instructions and exits 0, so the check can
 * ship before the first green run produces a floor to commit.
 */

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

interface FileCoverage {
  linesFound: number;
  linesHit: number;
  pct: number;
}

export interface CoverageSummary {
  total: FileCoverage;
  files: Record<string, FileCoverage>;
}

/** Merges lcov DA records: union of lines per file, a line is hit if any run hit it. */
export function mergeLcov(lcovTexts: string[]): CoverageSummary {
  // file -> line -> hit?
  const hits = new Map<string, Map<number, boolean>>();

  for (const text of lcovTexts) {
    let current: Map<number, boolean> | undefined;
    for (const rawLine of text.split('\n')) {
      const line = rawLine.trim();
      if (line.startsWith('SF:')) {
        const file = line.slice(3);
        current = hits.get(file) ?? new Map();
        hits.set(file, current);
      } else if (line.startsWith('DA:') && current) {
        const [lineNo, count] = line.slice(3).split(',').map(Number);
        if (lineNo === undefined || count === undefined) continue;
        if (!Number.isFinite(lineNo) || !Number.isFinite(count)) continue;
        current.set(lineNo, (current.get(lineNo) ?? false) || count > 0);
      } else if (line === 'end_of_record') {
        current = undefined;
      }
    }
  }

  const files: Record<string, FileCoverage> = {};
  let totalFound = 0;
  let totalHit = 0;
  for (const file of [...hits.keys()].sort()) {
    const lines = hits.get(file) as Map<number, boolean>;
    const linesFound = lines.size;
    const linesHit = [...lines.values()].filter(Boolean).length;
    totalFound += linesFound;
    totalHit += linesHit;
    files[file] = { linesFound, linesHit, pct: toPct(linesHit, linesFound) };
  }

  return {
    total: { linesFound: totalFound, linesHit: totalHit, pct: toPct(totalHit, totalFound) },
    files,
  };
}

function toPct(hit: number, found: number): number {
  return found === 0 ? 0 : Math.round((hit / found) * 10000) / 100;
}

function collectLcov(coverageDir: string): string[] {
  if (!existsSync(coverageDir)) return [];
  const texts: string[] = [];
  for (const entry of readdirSync(coverageDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const lcovPath = path.join(coverageDir, entry.name, 'lcov.info');
    if (existsSync(lcovPath)) texts.push(readFileSync(lcovPath, 'utf8'));
  }
  return texts;
}

export function mergeCoverageDir(coverageDir: string): CoverageSummary | undefined {
  const texts = collectLcov(coverageDir);
  if (texts.length === 0) return undefined;
  return mergeLcov(texts);
}

/** Floors are rounded down so a freshly-recorded run always passes its own floor. */
export function summaryToFloor(summary: CoverageSummary): Record<string, number> {
  const floor: Record<string, number> = { __total__: Math.floor(summary.total.pct) };
  for (const [file, cov] of Object.entries(summary.files)) {
    floor[file] = Math.floor(cov.pct);
  }
  return floor;
}

export interface DriftReport {
  regressions: string[];
  ok: boolean;
}

export function checkDrift(
  summary: CoverageSummary,
  floor: Record<string, number>
): DriftReport {
  const regressions: string[] = [];

  const totalFloor = floor.__total__;
  if (typeof totalFloor === 'number' && summary.total.pct < totalFloor) {
    regressions.push(
      `total: ${summary.total.pct}% is below the floor of ${totalFloor}%`
    );
  }
  for (const [file, filePct] of Object.entries(floor)) {
    if (file === '__total__') continue;
    const current = summary.files[file];
    if (!current) {
      regressions.push(`${file}: in the floor but absent from this run`);
    } else if (current.pct < filePct) {
      regressions.push(`${file}: ${current.pct}% is below the floor of ${filePct}%`);
    }
  }

  return { regressions, ok: regressions.length === 0 };
}

function main(): void {
  const [command, ...rest] = process.argv.slice(2);
  const args = new Map<string, string>();
  for (let i = 0; i < rest.length; i += 2) {
    const key = rest[i];
    const value = rest[i + 1];
    if (key?.startsWith('--') && value !== undefined) args.set(key.slice(2), value);
  }

  const coverageDir = args.get('coverage-dir');
  if (command !== 'check' || !coverageDir) {
    console.error(
      'usage: scenario-coverage.ts check --coverage-dir <dir> [--floor <scenarios.coverage.json>]'
    );
    process.exit(2);
  }

  const summary = mergeCoverageDir(coverageDir);
  if (!summary) {
    console.error(`no lcov.info files under ${coverageDir} — did the run use --coverage?`);
    process.exit(2);
  }

  mkdirSync(coverageDir, { recursive: true });
  const summaryPath = path.join(coverageDir, 'coverage-summary.json');
  writeFileSync(summaryPath, JSON.stringify(summary, null, 2) + '\n');
  const candidatePath = path.join(coverageDir, 'floor-candidate.json');
  writeFileSync(candidatePath, JSON.stringify(summaryToFloor(summary), null, 2) + '\n');

  console.log(
    `scenario coverage: ${summary.total.pct}% of ${summary.total.linesFound} lines ` +
      `across ${Object.keys(summary.files).length} files (summary: ${summaryPath})`
  );

  const floorPath = args.get('floor');
  if (!floorPath || !existsSync(floorPath)) {
    console.log(
      `no coverage floor committed yet — to bootstrap, commit ${candidatePath} as the floor file`
    );
    process.exit(0);
  }

  const floor = JSON.parse(readFileSync(floorPath, 'utf8')) as Record<string, number>;
  const drift = checkDrift(summary, floor);
  if (drift.ok) {
    console.log(`no drift: coverage meets the floor in ${floorPath}`);
    process.exit(0);
  }
  console.error(`coverage drift against ${floorPath}:`);
  for (const regression of drift.regressions) {
    console.error(`  - ${regression}`);
  }
  process.exit(1);
}

if (import.meta.main) {
  main();
}
