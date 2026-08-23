/**
 * Renders a scenario report into the run's job summary, so a dispatch shows
 * which scenarios passed without anyone opening the job log. Reads the report
 * the runner already writes rather than reparsing its stdout.
 */
import fs from 'node:fs';
import path from 'node:path';

// Imported rather than redeclared so the two shapes are at least written in one
// place. Nothing enforces it: no tsconfig covers this directory, and `import
// type` is erased before the file runs.
import type { RunReport } from '../../packages/tui/e2e_tests/scenario-runner/types';

const [reportDir, model, engine] = process.argv.slice(2);
if (!reportDir || !model || !engine) {
  console.error(
    'usage: model-sanity-summary.ts <report-dir> <model-id> <engine>'
  );
  process.exit(2);
}

const summaryPath = process.env.GITHUB_STEP_SUMMARY;
if (!summaryPath) {
  console.error('GITHUB_STEP_SUMMARY is unset; nothing to write');
  process.exit(0);
}

const reportFile = fs.existsSync(reportDir)
  ? fs
      .readdirSync(reportDir)
      .filter((f) => f.startsWith('scenario-report-') && f.endsWith('.json'))
      .sort()
      .pop()
  : undefined;

// Appended as each section is built rather than buffered to the end, so a
// surprise in the report's shape costs the sections after it and not the whole
// page — including the heading that says which leg this was.
const write = (...lines: string[]) =>
  fs.appendFileSync(summaryPath, `${lines.join('\n')}\n`);

write(`## ${engine} · \`${model}\``, '');

if (!reportFile) {
  // The runner writes its report before the scenarios' own exit code decides the
  // step, so a missing file means it never got that far — worth saying plainly
  // rather than rendering an empty table.
  write(
    `No scenario report in \`${reportDir}\`. The run failed before scenarios executed — check the log for the failing step.`,
    ''
  );
  process.exit(0);
}

const report: RunReport = JSON.parse(
  fs.readFileSync(path.join(reportDir, reportFile), 'utf8')
);
const ran = report.results;

write(
  `**${report.passed} passed · ${report.failed} failed** out of ${ran.length} run` +
    (report.skipped ? ` (${report.skipped} not selected)` : ''),
  '',
  '| | scenario | duration |',
  '|---|---|---|'
);

for (const r of ran) {
  write(
    `| ${r.passed ? '✅' : '❌'} | \`${r.scenario.id}\` — ${r.scenario.name} | ${(r.duration / 1000).toFixed(1)}s |`
  );
}
write('');

const failures = ran.filter((r) => !r.passed);
if (failures.length > 0) {
  write('### Failures', '');
  for (const r of failures) {
    write(`**\`${r.scenario.id}\`** — ${r.exitReason}`, '');
    if (r.error) {
      write('```', r.error, '```', '');
    }
    for (const v of (r.verifyResults ?? []).filter((v) => !v.passed)) {
      write(`- \`${v.predicate}\` → ${v.actual ?? 'failed'}`);
    }
    write('');
  }
}

console.log(`summary written for ${engine}: ${report.passed}/${ran.length}`);
