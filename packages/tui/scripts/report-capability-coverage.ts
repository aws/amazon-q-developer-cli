import { appendFile, mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import {
  calculateCapabilityCoverage,
  formatCapabilityCoverageMarkdown,
  loadCapabilityCoverageManifest,
  loadPassedTestEvidence,
  validateCapabilityEvidence,
} from '../src/test-utils/capability-coverage';

const [manifestArgument, outputArgument, junitArgument] = process.argv.slice(2);
if (!manifestArgument || !outputArgument || !junitArgument) {
  console.error(
    'Usage: bun run scripts/report-capability-coverage.ts <manifest> <output-dir> <junit-report>'
  );
  process.exit(2);
}

const manifestPath = resolve(manifestArgument);
const outputDirectory = resolve(outputArgument);
const manifest = await loadCapabilityCoverageManifest(manifestPath);
const passedTests = await loadPassedTestEvidence(resolve(junitArgument));
await validateCapabilityEvidence(manifest, process.cwd(), passedTests);

const report = calculateCapabilityCoverage(manifest);
const markdown = formatCapabilityCoverageMarkdown(report);
await mkdir(outputDirectory, { recursive: true });
await Promise.all([
  writeFile(
    resolve(outputDirectory, `${manifest.suite}.json`),
    `${JSON.stringify(report, null, 2)}\n`
  ),
  writeFile(resolve(outputDirectory, `${manifest.suite}.md`), markdown),
]);

if (process.env.GITHUB_STEP_SUMMARY) {
  await appendFile(process.env.GITHUB_STEP_SUMMARY, markdown);
}

console.log(
  `${manifest.suite}: ${report.covered}/${report.total} capabilities (${report.percent.toFixed(1)}%)`
);
