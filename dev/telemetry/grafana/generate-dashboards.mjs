import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { makeDetailedDashboard } from './definitions/detailed.mjs';
import { makeHealthDashboard } from './definitions/health.mjs';
import { validateDashboard } from './lib/grafana.mjs';

const generatorPath = fileURLToPath(import.meta.url);
const dashboardDir = join(dirname(generatorPath), 'dashboards');

export function buildDashboards() {
  return new Map([
    ['kiro-telemetry-local.json', makeDetailedDashboard()],
    ['kiro-telemetry-health-local.json', makeHealthDashboard()],
  ]);
}

export function generateDashboards({ check = false } = {}) {
  let changed = false;
  for (const [filename, dashboard] of buildDashboards()) {
    validateDashboard(dashboard);
    const path = join(dashboardDir, filename);
    const rendered = `${JSON.stringify(dashboard, null, 2)}\n`;
    if (check) {
      if (readFileSync(path, 'utf8') !== rendered) {
        console.error(
          `${filename} is stale; run bun dev/telemetry/grafana/generate-dashboards.mjs`
        );
        changed = true;
      }
    } else {
      writeFileSync(path, rendered);
      console.log(`generated ${filename}`);
    }
  }
  return !changed;
}

if (process.argv[1] && resolve(process.argv[1]) === generatorPath) {
  if (!generateDashboards({ check: process.argv.includes('--check') })) {
    process.exitCode = 1;
  }
}
