#!/usr/bin/env bun
/**
 * Compare two profile reports to find regressions/improvements.
 * Usage: bun run scripts/diff-profiles.ts <baseline.json> <current.json>
 */

import { readFileSync } from 'fs';

interface ProfileReport {
  timestamp: string;
  durationS: number;
  samples: number;
  functions: { name: string; selfTimeMs: number; totalTimeMs: number; calls: number; type: string }[];
}

const [baselinePath, currentPath] = process.argv.slice(2);

if (!baselinePath || !currentPath) {
  console.error('Usage: bun run scripts/diff-profiles.ts <baseline.json> <current.json>');
  process.exit(1);
}

const baseline: ProfileReport = JSON.parse(readFileSync(baselinePath, 'utf-8'));
const current: ProfileReport = JSON.parse(readFileSync(currentPath, 'utf-8'));

const baselineMap = new Map(baseline.functions.map(f => [f.name, f]));
const currentMap = new Map(current.functions.map(f => [f.name, f]));

console.log('\n📊 Profile Comparison\n');
console.log(`Baseline: ${baseline.timestamp} (${baseline.durationS.toFixed(1)}s)`);
console.log(`Current:  ${current.timestamp} (${current.durationS.toFixed(1)}s)\n`);

// Normalize by duration
const durationRatio = current.durationS / baseline.durationS;

console.log('Function                            | Base Self | Curr Self | Change');
console.log('------------------------------------|-----------|-----------|--------');

const changes: { name: string; baseMs: number; currMs: number; change: number }[] = [];

for (const [name, curr] of currentMap) {
  const base = baselineMap.get(name);
  if (base) {
    const normalizedBase = base.selfTimeMs / baseline.durationS;
    const normalizedCurr = curr.selfTimeMs / current.durationS;
    const change = ((normalizedCurr - normalizedBase) / normalizedBase) * 100;
    changes.push({ name, baseMs: base.selfTimeMs, currMs: curr.selfTimeMs, change });
  }
}

// Sort by absolute change
changes.sort((a, b) => Math.abs(b.change) - Math.abs(a.change));

for (const { name, baseMs, currMs, change } of changes.slice(0, 15)) {
  const changeStr = change > 0 ? `🔴 +${change.toFixed(0)}%` : `🟢 ${change.toFixed(0)}%`;
  console.log(
    `${name.slice(0, 35).padEnd(35)} | ${baseMs.toFixed(0).padStart(7)}ms | ${currMs.toFixed(0).padStart(7)}ms | ${changeStr}`
  );
}

// Summary
const regressions = changes.filter(c => c.change > 10);
const improvements = changes.filter(c => c.change < -10);

console.log('\n📋 Summary:');
console.log(`   Regressions (>10%): ${regressions.length}`);
console.log(`   Improvements (>10%): ${improvements.length}`);

if (regressions.length > 0) {
  console.log('\n🔴 Top Regressions:');
  for (const r of regressions.slice(0, 3)) {
    console.log(`   ${r.name.slice(0, 40)}: +${r.change.toFixed(0)}%`);
  }
}

if (improvements.length > 0) {
  console.log('\n🟢 Top Improvements:');
  for (const i of improvements.slice(0, 3)) {
    console.log(`   ${i.name.slice(0, 40)}: ${i.change.toFixed(0)}%`);
  }
}
