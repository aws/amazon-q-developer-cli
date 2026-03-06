#!/usr/bin/env bun
/**
 * Benchmark markdown parsing strategies used by the TUI stream renderer.
 *
 * Usage:
 *   bun run scripts/benchmark-markdown.ts
 *   bun run scripts/benchmark-markdown.ts --runs=5
 *   bun run scripts/benchmark-markdown.ts --json
 *   bun run scripts/benchmark-markdown.ts --verify
 *   bun run scripts/benchmark-markdown.ts --heavy
 *   bun run scripts/benchmark-markdown.ts --scale=0.5
 */

import { parseMarkdown, tryAppendMarkdownDelta } from '../src/utils/markdown.ts';

interface Scenario {
  name: string;
  description: string;
  deltas: string[];
}

interface ScenarioRunResult {
  fullMs: number;
  incrementalMs: number;
  incrementalHits: number;
  fallbackCount: number;
  mismatches: number;
}

interface ScenarioSummary {
  name: string;
  description: string;
  chunks: number;
  fullMsMedian: number;
  incrementalMsMedian: number;
  speedupX: number;
  incrementalHitRatePct: number;
  fallbackRatePct: number;
  mismatches: number;
}

function parseRunsArg(defaultRuns = 3): number {
  const runsArg = process.argv.find((arg) => arg.startsWith('--runs='));
  if (!runsArg) return defaultRuns;
  const value = Number.parseInt(runsArg.split('=')[1] || '', 10);
  if (!Number.isFinite(value) || value < 1) return defaultRuns;
  return value;
}

function parseScaleArg(defaultScale = 1): number {
  const scaleArg = process.argv.find((arg) => arg.startsWith('--scale='));
  if (!scaleArg) return defaultScale;
  const value = Number.parseFloat(scaleArg.split('=')[1] || '');
  if (!Number.isFinite(value) || value <= 0) return defaultScale;
  return value;
}

function median(values: number[]): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((left, right) => left - right);
  const middle = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) return sorted[middle] || 0;
  return ((sorted[middle - 1] || 0) + (sorted[middle] || 0)) / 2;
}

function buildPlainProseScenario(size = 12_000): Scenario {
  const deltas = Array.from({ length: size }, (_, index) => {
    if (index % 12 === 0) return ` chunk-${index} with extra prose and punctuation.`;
    if (index % 17 === 0) return `\nparagraph-${index}`;
    return ` token-${index}`;
  });
  return {
    name: 'plain-prose-heavy',
    description: 'Mostly plain text append-only streaming',
    deltas,
  };
}

function buildMixedMarkdownScenario(size = 6_000): Scenario {
  const deltas: string[] = [];
  for (let index = 0; index < size; index++) {
    if (index % 700 === 0) {
      deltas.push(`\n## Section ${Math.floor(index / 700)}\n`);
    } else if (index % 251 === 0) {
      deltas.push('\n```ts\nconst value = 1;\n');
    } else if (index % 251 === 8) {
      deltas.push('console.log(value);\n```\n');
    } else if (index % 181 === 0) {
      deltas.push(`\n- list item ${index}\n`);
    } else if (index % 311 === 0) {
      deltas.push('\n| col-a | col-b |\n|---|---|\n| a | b |\n');
    } else {
      deltas.push(` word-${index}`);
    }
  }
  return {
    name: 'mixed-markdown-heavy',
    description: 'Interleaved headers/lists/code fences/tables',
    deltas,
  };
}

function buildCodeFenceStreamingScenario(blocks = 1_500): Scenario {
  const deltas: string[] = [];
  for (let index = 0; index < blocks; index++) {
    deltas.push('```');
    deltas.push('ts');
    deltas.push('\n');
    deltas.push(`const value${index} = ${index};\n`);
    deltas.push(`console.log(value${index});\n`);
    deltas.push('```');
    deltas.push('\n');
    deltas.push(`After block ${index}\n`);
  }
  return {
    name: 'code-fence-streaming',
    description: 'Fence language/code streamed across many small deltas',
    deltas,
  };
}

function buildBlockTransitionScenario(blocks = 2_000): Scenario {
  const deltas: string[] = [];
  for (let index = 0; index < blocks; index++) {
    if (index % 2 === 0) {
      deltas.push(`\n## Header ${index}\n`);
      deltas.push(`Body ${index}`);
      deltas.push('\n');
    } else {
      deltas.push(`\n- item-${index}`);
      deltas.push(` tail-${index}\n`);
      deltas.push(`paragraph-${index}`);
    }
  }
  return {
    name: 'block-transitions',
    description: 'Frequent transitions between block and text lines',
    deltas,
  };
}

function runScenarioOnce(scenario: Scenario, verifyParity: boolean): ScenarioRunResult {
  let fullContent = '';
  const fullStart = performance.now();
  for (const delta of scenario.deltas) {
    fullContent += delta;
    parseMarkdown(fullContent);
  }
  const fullMs = performance.now() - fullStart;

  let incrementalContent = '';
  let cacheContent = '';
  let cacheSegments: ReturnType<typeof parseMarkdown> = [];
  let incrementalHits = 0;
  let fallbackCount = 0;
  let mismatches = 0;

  const incrementalStart = performance.now();
  for (const delta of scenario.deltas) {
    incrementalContent += delta;

    if (!cacheContent) {
      cacheSegments = parseMarkdown(incrementalContent);
      cacheContent = incrementalContent;
      fallbackCount++;
      if (verifyParity) {
        const full = parseMarkdown(incrementalContent);
        if (JSON.stringify(cacheSegments) !== JSON.stringify(full)) {
          mismatches++;
        }
      }
      continue;
    }

    const appended = tryAppendMarkdownDelta(cacheSegments, delta, cacheContent);
    if (appended) {
      cacheSegments = appended;
      cacheContent = incrementalContent;
      incrementalHits++;
    } else {
      cacheSegments = parseMarkdown(incrementalContent);
      cacheContent = incrementalContent;
      fallbackCount++;
    }

    if (verifyParity) {
      const full = parseMarkdown(incrementalContent);
      if (JSON.stringify(cacheSegments) !== JSON.stringify(full)) {
        mismatches++;
      }
    }
  }
  const incrementalMs = performance.now() - incrementalStart;

  return {
    fullMs,
    incrementalMs,
    incrementalHits,
    fallbackCount,
    mismatches,
  };
}

function summarizeScenario(
  scenario: Scenario,
  runs: ScenarioRunResult[]
): ScenarioSummary {
  const fullValues = runs.map((run) => run.fullMs);
  const incrementalValues = runs.map((run) => run.incrementalMs);
  const fullMsMedian = median(fullValues);
  const incrementalMsMedian = median(incrementalValues);

  const totalChunks = scenario.deltas.length * runs.length;
  const totalHits = runs.reduce((sum, run) => sum + run.incrementalHits, 0);
  const totalFallbacks = runs.reduce((sum, run) => sum + run.fallbackCount, 0);
  const totalMismatches = runs.reduce((sum, run) => sum + run.mismatches, 0);

  return {
    name: scenario.name,
    description: scenario.description,
    chunks: scenario.deltas.length,
    fullMsMedian: Number(fullMsMedian.toFixed(2)),
    incrementalMsMedian: Number(incrementalMsMedian.toFixed(2)),
    speedupX: Number((fullMsMedian / incrementalMsMedian).toFixed(2)),
    incrementalHitRatePct: Number(((totalHits / totalChunks) * 100).toFixed(2)),
    fallbackRatePct: Number(((totalFallbacks / totalChunks) * 100).toFixed(2)),
    mismatches: totalMismatches,
  };
}

function printTable(summaries: ScenarioSummary[], runs: number, verifyParity: boolean): void {
  console.log('\n📊 Markdown Incremental Benchmark');
  console.log(`   Runs per scenario: ${runs}`);
  console.log(`   Parity check: ${verifyParity ? 'enabled' : 'disabled'}\n`);
  console.log(
    'Scenario               | Full ms | Incr ms | Speedup | Hit Rate | Fallback | Mismatches'
  );
  console.log(
    '-----------------------|---------|---------|---------|----------|----------|-----------'
  );

  for (const summary of summaries) {
    const row = [
      summary.name.padEnd(22),
      `${summary.fullMsMedian.toFixed(2).padStart(7)}`,
      `${summary.incrementalMsMedian.toFixed(2).padStart(7)}`,
      `${summary.speedupX.toFixed(2).padStart(6)}x`,
      `${summary.incrementalHitRatePct.toFixed(2).padStart(7)}%`,
      `${summary.fallbackRatePct.toFixed(2).padStart(7)}%`,
      `${String(summary.mismatches).padStart(9)}`,
    ].join(' | ');
    console.log(row);
  }

  console.log('\nScenarios:');
  for (const summary of summaries) {
    console.log(`- ${summary.name} (${summary.chunks} chunks): ${summary.description}`);
  }
}

function main(): void {
  const runs = parseRunsArg();
  const outputJson = process.argv.includes('--json');
  const verifyParity = process.argv.includes('--verify');
  const heavyMode = process.argv.includes('--heavy');
  const scale = parseScaleArg();

  const sizeMultiplier = heavyMode ? 4 * scale : scale;
  const scaled = (size: number): number => Math.max(1, Math.floor(size * sizeMultiplier));

  const scenarios: Scenario[] = [
    buildPlainProseScenario(scaled(2_500)),
    buildMixedMarkdownScenario(scaled(1_800)),
    buildCodeFenceStreamingScenario(scaled(400)),
    buildBlockTransitionScenario(scaled(900)),
  ];

  const summaries = scenarios.map((scenario) => {
    const runResults = Array.from({ length: runs }, () =>
      runScenarioOnce(scenario, verifyParity)
    );
    return summarizeScenario(scenario, runResults);
  });

  if (outputJson) {
    console.log(JSON.stringify({ runs, verifyParity, summaries }, null, 2));
    return;
  }

  if (heavyMode) {
    console.log('⚙️  Running in --heavy mode (larger workloads).');
  }
  printTable(summaries, runs, verifyParity);
}

main();
