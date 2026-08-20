#!/usr/bin/env bun

import * as fs from 'node:fs';
import * as path from 'node:path';
import { parseArgs } from 'node:util';

interface ScenarioReport {
  backendId: string;
  engine: string;
  total: number;
  passed: number;
  failed: number;
  skipped: number;
}

interface VisualFrame {
  status: 'passed' | 'failed';
}

interface VisualCoverageSummary {
  totalStories: number;
  coveredStories: number;
  totalVariants: number;
  executedVariants: number;
  totalComponents: number;
  coveredComponents: number;
}

interface VisualManifest {
  version: 1 | 2;
  suite: string;
  generatedAt: string;
  frames: VisualFrame[];
  coverage?: VisualCoverageSummary;
}

interface ScenarioReportSummary {
  artifact: string;
  relativePath: string;
  backendId: string;
  engine: string;
  total: number;
  passed: number;
  failed: number;
  skipped: number;
}

interface VisualManifestSummary {
  artifact: string;
  relativePath: string;
  suite: string;
  generatedAt: string;
  totalFrames: number;
  passedFrames: number;
  failedFrames: number;
  coverage?: VisualCoverageSummary;
  reportPath?: string;
}

export interface VerificationArtifactSummary {
  scenarioReports: ScenarioReportSummary[];
  visualSuites: VisualManifestSummary[];
}

interface ScenarioReportCandidate extends ScenarioReportSummary {
  sequence: number;
}

interface VerificationPublication {
  scenarioLinks: Map<string, string>;
  visualLinks: Map<string, string>;
}

function normalizeRelativePath(value: string): string {
  return value.replaceAll(path.sep, '/');
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function formatCoverage(covered: number, total: number): string {
  const percent = total === 0 ? 0 : (covered / total) * 100;
  return `${covered}/${total} (${percent.toFixed(1)}%)`;
}

function visualCoverageCells(
  coverage: VisualCoverageSummary | undefined
): [string, string, string] {
  if (!coverage) {
    return ['n/a', 'n/a', 'n/a'];
  }
  return [
    formatCoverage(coverage.coveredStories, coverage.totalStories),
    formatCoverage(coverage.executedVariants, coverage.totalVariants),
    formatCoverage(coverage.coveredComponents, coverage.totalComponents),
  ];
}

function parseVisualCoverage(
  value: VisualCoverageSummary | undefined
): VisualCoverageSummary | undefined {
  if (!value) {
    return undefined;
  }
  const counts = [
    value.totalStories,
    value.coveredStories,
    value.totalVariants,
    value.executedVariants,
    value.totalComponents,
    value.coveredComponents,
  ];
  if (
    counts.some(
      (count) =>
        typeof count !== 'number' || !Number.isSafeInteger(count) || count < 0
    ) ||
    value.coveredStories > value.totalStories ||
    value.executedVariants > value.totalVariants ||
    value.coveredComponents > value.totalComponents
  ) {
    return undefined;
  }
  return {
    totalStories: value.totalStories,
    coveredStories: value.coveredStories,
    totalVariants: value.totalVariants,
    executedVariants: value.executedVariants,
    totalComponents: value.totalComponents,
    coveredComponents: value.coveredComponents,
  };
}

function walkFiles(rootDir: string): string[] {
  const entries = fs.readdirSync(rootDir, { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries) {
    const fullPath = path.join(rootDir, entry.name);
    if (entry.isDirectory()) {
      files.push(...walkFiles(fullPath));
      continue;
    }
    files.push(fullPath);
  }
  return files;
}

function artifactName(rootDir: string, filePath: string): string {
  const relative = path.relative(rootDir, filePath);
  const [artifact = '.'] = relative.split(path.sep);
  return artifact;
}

function readJson<T>(filePath: string): T {
  return JSON.parse(fs.readFileSync(filePath, 'utf8')) as T;
}

function reportSequence(filePath: string): number {
  const match = path.basename(filePath).match(/-(\d+)\.json$/);
  if (match) {
    return Number(match[1]);
  }
  return fs.statSync(filePath).mtimeMs;
}

function emptySummary(): VerificationArtifactSummary {
  return {
    scenarioReports: [],
    visualSuites: [],
  };
}

function copyFile(sourcePath: string, targetPath: string): void {
  fs.mkdirSync(path.dirname(targetPath), { recursive: true });
  fs.copyFileSync(sourcePath, targetPath);
}

function copyDirectory(sourceDir: string, targetDir: string): void {
  fs.mkdirSync(targetDir, { recursive: true });
  for (const entry of fs.readdirSync(sourceDir, { withFileTypes: true })) {
    const sourcePath = path.join(sourceDir, entry.name);
    const targetPath = path.join(targetDir, entry.name);
    if (entry.isDirectory()) {
      copyDirectory(sourcePath, targetPath);
      continue;
    }
    copyFile(sourcePath, targetPath);
  }
}

export function collectVerificationArtifacts(
  rootDir: string
): VerificationArtifactSummary {
  if (!fs.existsSync(rootDir)) {
    return emptySummary();
  }

  const scenarioReportsByLane = new Map<string, ScenarioReportCandidate>();
  const visualSuites: VisualManifestSummary[] = [];

  for (const filePath of walkFiles(rootDir)) {
    const baseName = path.basename(filePath);
    const relativePath = normalizeRelativePath(
      path.relative(rootDir, filePath)
    );
    const artifact = artifactName(rootDir, filePath);

    if (baseName.startsWith('scenario-report-') && baseName.endsWith('.json')) {
      const report = readJson<ScenarioReport>(filePath);
      const candidate: ScenarioReportCandidate = {
        artifact,
        relativePath,
        backendId: report.backendId,
        engine: report.engine,
        total: report.total,
        passed: report.passed,
        failed: report.failed,
        skipped: report.skipped,
        sequence: reportSequence(filePath),
      };
      const laneKey = `${artifact}:${report.backendId}:${report.engine}`;
      const existing = scenarioReportsByLane.get(laneKey);
      if (!existing || candidate.sequence >= existing.sequence) {
        scenarioReportsByLane.set(laneKey, candidate);
      }
      continue;
    }

    if (baseName !== 'manifest.json') {
      continue;
    }

    const manifest = readJson<VisualManifest>(filePath);
    if (manifest.version !== 1 && manifest.version !== 2) {
      console.warn(
        `Skipping unsupported visual manifest version at ${relativePath}: ${String(manifest.version)}`
      );
      continue;
    }
    if (!Array.isArray(manifest.frames)) {
      continue;
    }

    const passedFrames = manifest.frames.filter(
      (frame) => frame.status === 'passed'
    ).length;
    const failedFrames = manifest.frames.length - passedFrames;
    const coverage = parseVisualCoverage(manifest.coverage);
    const reportPath = fs.existsSync(
      path.join(path.dirname(filePath), 'index.html')
    )
      ? normalizeRelativePath(
          path.relative(
            rootDir,
            path.join(path.dirname(filePath), 'index.html')
          )
        )
      : undefined;

    visualSuites.push({
      artifact,
      relativePath,
      suite: manifest.suite,
      generatedAt: manifest.generatedAt,
      totalFrames: manifest.frames.length,
      passedFrames,
      failedFrames,
      ...(coverage ? { coverage } : {}),
      ...(reportPath ? { reportPath } : {}),
    });
  }

  const scenarioReports = Array.from(scenarioReportsByLane.values())
    .map(({ sequence: _sequence, ...report }) => report)
    .sort((a, b) => a.relativePath.localeCompare(b.relativePath));
  visualSuites.sort((a, b) => a.relativePath.localeCompare(b.relativePath));
  return { scenarioReports, visualSuites };
}

function stageVerificationArtifacts(
  summary: VerificationArtifactSummary,
  rootDir: string,
  outDir: string
): VerificationPublication {
  const artifactRoot = path.join(outDir, 'artifacts');
  const scenarioLinks = new Map<string, string>();
  const visualLinks = new Map<string, string>();
  const copiedVisualRoots = new Set<string>();

  for (const report of summary.scenarioReports) {
    const sourcePath = path.join(rootDir, report.relativePath);
    const targetPath = path.join(artifactRoot, report.relativePath);
    if (fs.existsSync(sourcePath)) {
      copyFile(sourcePath, targetPath);
      scenarioLinks.set(
        report.relativePath,
        normalizeRelativePath(path.relative(outDir, targetPath))
      );
    }
  }

  for (const report of summary.visualSuites) {
    const target = report.reportPath ?? report.relativePath;
    const sourcePath = path.join(rootDir, target);
    const targetPath = path.join(artifactRoot, target);

    if (report.reportPath) {
      const sourceDir = path.dirname(sourcePath);
      const targetDir = path.dirname(targetPath);
      if (fs.existsSync(sourceDir) && !copiedVisualRoots.has(sourceDir)) {
        copyDirectory(sourceDir, targetDir);
        copiedVisualRoots.add(sourceDir);
      }
    } else if (fs.existsSync(sourcePath)) {
      copyFile(sourcePath, targetPath);
    }

    if (fs.existsSync(targetPath)) {
      visualLinks.set(
        report.relativePath,
        normalizeRelativePath(path.relative(outDir, targetPath))
      );
    }
  }

  return { scenarioLinks, visualLinks };
}

function generateMarkdown(
  summary: VerificationArtifactSummary,
  publication: VerificationPublication
): string {
  const scenarioFailed = summary.scenarioReports.reduce(
    (count, report) => count + report.failed,
    0
  );
  const visualFailed = summary.visualSuites.reduce(
    (count, report) => count + report.failedFrames,
    0
  );

  const lines = [
    '## Verification Summary',
    '',
    `- Scenario report artifacts: ${summary.scenarioReports.length}`,
    `- Visual suite artifacts: ${summary.visualSuites.length}`,
    `- Scenario failures: ${scenarioFailed}`,
    `- Visual frame failures: ${visualFailed}`,
    '',
  ];

  if (summary.scenarioReports.length > 0) {
    lines.push(
      '| Scenario Artifact | Backend | Engine | Passed | Failed | Skipped | Bundle Path |'
    );
    lines.push('|---|---|---:|---:|---:|---:|---|');
    for (const report of summary.scenarioReports) {
      const bundlePath =
        publication.scenarioLinks.get(report.relativePath) ??
        report.relativePath;
      lines.push(
        `| ${report.artifact} | ${report.backendId} | ${report.engine} | ${report.passed} | ${report.failed} | ${report.skipped} | \`${bundlePath}\` |`
      );
    }
    lines.push('');
  }

  if (summary.visualSuites.length > 0) {
    lines.push(
      '| Visual Artifact | Suite | Passed Frames | Failed Frames | Story Coverage | Variant Execution | Component Coverage | Bundle Path |'
    );
    lines.push('|---|---|---:|---:|---:|---:|---:|---|');
    for (const report of summary.visualSuites) {
      const bundlePath =
        publication.visualLinks.get(report.relativePath) ??
        report.reportPath ??
        report.relativePath;
      const coverage = visualCoverageCells(report.coverage);
      lines.push(
        `| ${report.artifact} | ${report.suite} | ${report.passedFrames} | ${report.failedFrames} | ${coverage.join(' | ')} | \`${bundlePath}\` |`
      );
    }
    lines.push(
      '_Story coverage counts a story once any variant runs. Component coverage is a static reachability estimate; the linked artifact lists every covered and missed story, variant, and component._'
    );
    lines.push('');
  }

  return `${lines.join('\n')}\n`;
}

function generateHtml(
  summary: VerificationArtifactSummary,
  publication: VerificationPublication
): string {
  const scenarioRows = summary.scenarioReports
    .map((report) => {
      const href = publication.scenarioLinks.get(report.relativePath);
      const linkCell = href
        ? `<a href="${escapeHtml(href)}">json</a>`
        : escapeHtml(report.relativePath);
      return `<tr>
  <td>${escapeHtml(report.artifact)}</td>
  <td>${escapeHtml(report.backendId)}</td>
  <td>${escapeHtml(report.engine)}</td>
  <td>${report.passed}</td>
  <td>${report.failed}</td>
  <td>${report.skipped}</td>
  <td>${linkCell}</td>
</tr>`;
    })
    .join('\n');

  const visualRows = summary.visualSuites
    .map((report) => {
      const href = publication.visualLinks.get(report.relativePath);
      const linkCell = href
        ? `<a href="${escapeHtml(href)}">artifact</a>`
        : escapeHtml(report.reportPath ?? report.relativePath);
      const coverage = visualCoverageCells(report.coverage);
      return `<tr>
  <td>${escapeHtml(report.artifact)}</td>
  <td>${escapeHtml(report.suite)}</td>
  <td>${report.passedFrames}</td>
  <td>${report.failedFrames}</td>
  ${coverage.map((value) => `<td>${value}</td>`).join('\n  ')}
  <td>${linkCell}</td>
</tr>`;
    })
    .join('\n');

  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Verification Summary</title>
<style>
body{font-family:system-ui,sans-serif;margin:0;background:#0b1220;color:#dbe4ff}
main{max-width:1200px;margin:0 auto;padding:24px}
h1,h2{margin:0 0 12px}
section{margin-top:24px;background:#111827;border:1px solid #334155;border-radius:12px;padding:16px}
section{overflow-x:auto}
table{width:100%;border-collapse:collapse;white-space:nowrap}
th,td{text-align:left;padding:10px 12px;border-bottom:1px solid #334155}
th{color:#93c5fd;font-size:12px;text-transform:uppercase;letter-spacing:.04em}
a{color:#7dd3fc}
.empty{color:#94a3b8}
</style>
</head>
<body>
<main>
  <h1>Verification Summary</h1>
  <section>
    <h2>Scenario Reports</h2>
    ${
      summary.scenarioReports.length > 0
        ? `<table><thead><tr><th>Artifact</th><th>Backend</th><th>Engine</th><th>Passed</th><th>Failed</th><th>Skipped</th><th>Link</th></tr></thead><tbody>${scenarioRows}</tbody></table>`
        : '<p class="empty">No scenario reports found.</p>'
    }
  </section>
  <section>
    <h2>Visual Suites</h2>
    ${
      summary.visualSuites.length > 0
        ? `<p class="explanation">Story coverage counts a story once any variant runs. Component coverage is a static reachability estimate; open the artifact for every covered and missed story, variant, and component.</p><table><thead><tr><th>Artifact</th><th>Suite</th><th>Passed Frames</th><th>Failed Frames</th><th>Story Coverage</th><th>Variant Execution</th><th>Component Coverage</th><th>Link</th></tr></thead><tbody>${visualRows}</tbody></table>`
        : '<p class="empty">No visual suite artifacts found.</p>'
    }
  </section>
</main>
</body>
</html>`;
}

export function writeVerificationReport(
  rootDir: string,
  outDir: string
): VerificationArtifactSummary {
  const summary = collectVerificationArtifacts(rootDir);
  fs.mkdirSync(outDir, { recursive: true });
  const publication = stageVerificationArtifacts(summary, rootDir, outDir);
  fs.writeFileSync(
    path.join(outDir, 'summary.json'),
    JSON.stringify(summary, null, 2)
  );
  fs.writeFileSync(
    path.join(outDir, 'summary.md'),
    generateMarkdown(summary, publication)
  );
  fs.writeFileSync(
    path.join(outDir, 'index.html'),
    generateHtml(summary, publication)
  );
  return summary;
}

function parseCli(): { rootDir: string; outDir: string } {
  const { values } = parseArgs({
    options: {
      root: { type: 'string' },
      'out-dir': { type: 'string' },
    },
    strict: true,
    allowPositionals: false,
  });

  const rootDir = path.resolve(values.root ?? 'verification-artifacts');
  const outDir = path.resolve(
    values['out-dir'] ?? path.join(rootDir, 'summary')
  );
  return { rootDir, outDir };
}

async function main(): Promise<void> {
  const { rootDir, outDir } = parseCli();
  writeVerificationReport(rootDir, outDir);
  console.log(`Verification summary: ${path.join(outDir, 'index.html')}`);
}

if (import.meta.main) {
  await main();
}
