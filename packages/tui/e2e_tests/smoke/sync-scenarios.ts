#!/usr/bin/env bun
/**
 * sync-scenarios.ts — Compare scenarios.json against the docs slash-commands
 * reference page and detect missing scenarios. If new ones are found, update
 * scenarios.json and optionally create a PR.
 *
 * Usage:
 *   bun run e2e_tests/smoke/sync-scenarios.ts                    # dry-run
 *   bun run e2e_tests/smoke/sync-scenarios.ts --apply             # update file
 *   bun run e2e_tests/smoke/sync-scenarios.ts --apply --pr        # update + PR
 *   bun run e2e_tests/smoke/sync-scenarios.ts --docs-path <path>  # custom docs
 */

import * as fs from 'fs';
import * as path from 'path';
import { execSync } from 'child_process';

const SCENARIOS_PATH = path.join(__dirname, 'scenarios.json');
const DEFAULT_DOCS_PATH = path.join(
  process.env.HOME ?? '~',
  'workplace/kiro-docs/contents/docs/cli/reference/slash-commands/index.mdx'
);

// Parse args
const args = process.argv.slice(2);
const apply = args.includes('--apply');
const createPr = args.includes('--pr');
const docsIdx = args.indexOf('--docs-path');
const docsPath: string = docsIdx >= 0 ? (args[docsIdx + 1] ?? DEFAULT_DOCS_PATH) : DEFAULT_DOCS_PATH;

interface Scenario {
  id: string;
  name: string;
  category: string;
  description: string;
  docRef: string;
  steps: string[];
  verify: string[];
}

interface Manifest {
  version: string;
  generatedFrom: string;
  scenarios: Scenario[];
}

// ── Parse docs for slash commands ───────────────────────────────────────────

function parseDocsCommands(mdxContent: string): Array<{ command: string; description: string }> {
  const commands: Array<{ command: string; description: string }> = [];
  const lines = mdxContent.split('\n');

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const match = line.match(/^###\s+`(\/\w[\w-]*)`/);
    if (match) {
      const command = match[1]!;
      // Grab the next non-empty line as description
      let desc = '';
      for (let j = i + 1; j < Math.min(i + 5, lines.length); j++) {
        const trimmed = lines[j]!.trim();
        if (trimmed && !trimmed.startsWith('#') && !trimmed.startsWith('```')) {
          desc = trimmed;
          break;
        }
      }
      commands.push({ command, description: desc });
    }
  }
  return commands;
}

// ── Main ────────────────────────────────────────────────────────────────────

const manifest: Manifest = JSON.parse(fs.readFileSync(SCENARIOS_PATH, 'utf8'));
const existingIds = new Set(manifest.scenarios.map(s => s.id));

// Commands already covered (extract the slash command from scenario id/steps)
const coveredCommands = new Set<string>();
for (const s of manifest.scenarios) {
  // Extract from steps like "type:/help"
  for (const step of s.steps) {
    const m = step.match(/^type:(\/\w[\w-]*)/);
    if (m?.[1]) coveredCommands.add(m[1].split(' ')[0]!);
  }
}

if (!fs.existsSync(docsPath)) {
  console.error(`Docs not found at ${docsPath}`);
  process.exit(1);
}

const docsContent = fs.readFileSync(docsPath, 'utf8');
const docsCommands = parseDocsCommands(docsContent);

console.log(`📄 Found ${docsCommands.length} commands in docs`);
console.log(`📋 Existing scenarios: ${manifest.scenarios.length} (covering ${coveredCommands.size} commands)`);

const newScenarios: Scenario[] = [];

for (const cmd of docsCommands) {
  if (coveredCommands.has(cmd.command)) continue;

  const id = `slash-${cmd.command.slice(1).replace(/\s+/g, '-')}`;
  if (existingIds.has(id)) continue;

  const scenario: Scenario = {
    id,
    name: `${cmd.command} command`,
    category: 'slash-commands',
    description: cmd.description.slice(0, 100),
    docRef: `/docs/cli/reference/slash-commands#${cmd.command.slice(1)}`,
    steps: [`type:${cmd.command}`, 'enter'],
    verify: [`screen.contains:${cmd.command.slice(1)}`],
  };

  newScenarios.push(scenario);
}

if (newScenarios.length === 0) {
  console.log('✅ All doc commands are covered by scenarios');
  process.exit(0);
}

console.log(`\n🆕 Found ${newScenarios.length} new commands not in scenarios:`);
for (const s of newScenarios) {
  console.log(`   - ${s.id}: ${s.name}`);
}

if (!apply) {
  console.log('\nRun with --apply to update scenarios.json');
  process.exit(0);
}

// Update manifest
manifest.scenarios.push(...newScenarios);
fs.writeFileSync(SCENARIOS_PATH, JSON.stringify(manifest, null, 2) + '\n');
console.log(`\n✏️  Updated ${SCENARIOS_PATH} (+${newScenarios.length} scenarios)`);

if (createPr) {
  const branch = `smoke-test-sync-${Date.now()}`;
  try {
    execSync(`git checkout -b ${branch}`, { cwd: path.join(__dirname, '../../../..'), stdio: 'inherit' });
    execSync(`git add ${SCENARIOS_PATH}`, { cwd: path.join(__dirname, '../../../..'), stdio: 'inherit' });
    execSync(`git commit -m "test(smoke): add ${newScenarios.length} new scenarios from docs"`, { cwd: path.join(__dirname, '../../../..'), stdio: 'inherit' });
    execSync(`gh pr create --repo kiro-team/kiro-cli --title "test(smoke): sync scenarios with docs (+${newScenarios.length})" --body "Auto-detected ${newScenarios.length} new slash commands in docs that lack smoke test coverage:\\n\\n${newScenarios.map(s => '- ' + s.name).join('\\n')}"`, { cwd: path.join(__dirname, '../../../..'), stdio: 'inherit' });
    console.log('✅ PR created');
  } catch (err: any) {
    console.error('Failed to create PR:', err.message);
  }
}
