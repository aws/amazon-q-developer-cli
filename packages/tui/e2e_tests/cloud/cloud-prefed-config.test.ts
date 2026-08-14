/**
 * Pre-fed LOCAL config in cloud sessions — E2E against the real TUI +
 * published KAS + mock BFF.
 *
 * Config-reading surfaces split by scope in a cloud session: the sandbox
 * owns agents/MCP/tools/hooks (pushed over the downlink or fetched via
 * relayed ext-methods), while `.kiro/specs`, `.kiro/agents`,
 * `.kiro/workflows`, and `mcp.json` on the LOCAL machine describe a
 * workspace the session is not running on. Each test here SEEDS real local
 * config first — a gate or empty-state assertion is only meaningful when
 * there is actual local state that would leak if the scoping regressed —
 * then drives the surface in one cloud session:
 *
 *   /agent picker: sandbox-owned surface lists the downlink-pushed set;
 *     seeded local agent never listed; create/edit refuse       (test 1)
 *   /workflow: localOnly commands stay silent in cloud even with
 *     a seeded recipe (dispatcher refusal, kiro-agent #178)     (test 1)
 *   /mcp panel: seeded workspace mcp.json server name never
 *     renders as sandbox state (#3690 class, pre-fed variant)   (test 2)
 *
 * /spec's pre-fed gate lives in cloud-spec.test.ts. The steering/prompts
 * slices currently have NO cloud reset (local entries persist into a cloud
 * session's /prompts picker) — that is an open scope-mismatch finding, not
 * pinned here because the current behavior is the bug.
 */
import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { CloudHarness } from './CloudTestCase';

const KAS_SERVER = path.join(
  __dirname,
  '../../node_modules/@kiro/agent/dist/server/acp-server.js'
);
const kasServerMissing =
  process.platform !== 'win32' && !fs.existsSync(KAS_SERVER);
if (kasServerMissing) {
  console.warn(
    `SKIPPING cloud E2E: published KAS server not found at ${KAS_SERVER}. ` +
      'Run ./scripts/codeartifact-login.sh && bun install to enable these tests.'
  );
}
const skip = process.platform === 'win32' || kasServerMissing;

const BOOT_TIMEOUT = 60_000;

/**
 * Workflows take two independent conditions: the rollout has to reach the
 * user AND the user has to opt in. Both `/workflow` probes below need the
 * commands to exist at all — an opted-out run filters them out of the
 * command set, so the cloud absence assertion would hold for the wrong
 * reason and the local control could never open the picker. The rollout
 * half is stated rather than inherited from the launcher's own resolution,
 * which the merge at launch treats as an extra rather than a replacement.
 */
const WORKFLOWS_OPT_IN = {
  settings: { 'chat.enableWorkflows': true },
  env: { KIRO_ENABLED_FEATURES: '["workflows"]' },
} as const;

/** Type a line one key at a time (autocomplete-safe) and submit. */
async function typeLine(
  tc: NonNullable<CloudHarness['testCase']>,
  text: string
): Promise<void> {
  for (const ch of text) {
    await tc.sendKeys(ch);
    await tc.sleepMs(50);
  }
  await tc.sleepMs(300);
  await tc.pressEnter();
}

/**
 * Seed a workspace with the local config surfaces this suite probes:
 * a workspace agent profile, a workflow recipe, and an MCP server config.
 * Names are chosen to be un-typeable-by-accident so a screen grep for them
 * can only be satisfied by a real config read.
 */
function seedConfigWorkspace(): string {
  const ws = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-e2e-prefed-ws-'));

  const agentsDir = path.join(ws, '.kiro', 'agents');
  fs.mkdirSync(agentsDir, { recursive: true });
  fs.writeFileSync(
    path.join(agentsDir, 'localhero.json'),
    JSON.stringify({
      name: 'LOCALHERO_AGENT_PROBE',
      description: 'workspace agent that must not leak into cloud',
      prompt: 'You are a local test agent.',
    })
  );

  // Recipe FILENAME equals the probe string: the /workflow command below
  // references the file stem, so any echo/monitor/error path that surfaces
  // either the file reference or the recipe name trips the same probe.
  const workflowsDir = path.join(ws, '.kiro', 'workflows');
  fs.mkdirSync(workflowsDir, { recursive: true });
  fs.writeFileSync(
    path.join(workflowsDir, 'LOCALFLOW_RECIPE_PROBE.workflow.json'),
    JSON.stringify({
      name: 'LOCALFLOW_RECIPE_PROBE',
      description: 'workspace workflow recipe',
      steps: [],
    })
  );

  const settingsDir = path.join(ws, '.kiro', 'settings');
  fs.mkdirSync(settingsDir, { recursive: true });
  fs.writeFileSync(
    path.join(settingsDir, 'mcp.json'),
    JSON.stringify({
      mcpServers: {
        LOCALMCP_SERVER_PROBE: { command: 'true', args: [] },
      },
    })
  );

  return ws;
}

describe('cloud sessions — pre-fed local config never leaks (mock BFF)', () => {
  let harness: CloudHarness | null = null;
  const tempDirs: string[] = [];

  afterEach(async () => {
    if (harness) {
      await harness.cleanup();
      harness = null;
    }
    for (const dir of tempDirs.splice(0)) {
      fs.rmSync(dir, { recursive: true, force: true });
    }
  });

  it.skipIf(skip)(
    '/agent surfaces are sandbox-owned: seeded local agent never lists; create/edit refuse; /workflow stays silent',
    async () => {
      const ws = seedConfigWorkspace();
      tempDirs.push(ws);
      harness = await CloudHarness.launch({
        testName: 'cloud-prefed-agent',
        cwd: ws,
        ...WORKFLOWS_OPT_IN,
      });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      // /agent picker: the sandbox owns the agent surface. The KAS server
      // pushes its bundled set over the downlink, so the picker opens on
      // those — and must NOT merge in the seeded workspace profile the
      // way a local session would. The Default row is anchored by its
      // picker-specific Bundled source tag (the footer also says
      // "Default", so a bare toContain('Default') would be vacuous).
      await typeLine(tc, '/agent');
      await tc.waitForText('Select agent', 15_000);
      let snapshot = tc.getSnapshotFormatted();
      expect(snapshot).toMatch(/Default\s+Bundled/);
      expect(snapshot).not.toContain('LOCALHERO_AGENT_PROBE');
      await tc.pressEscape();
      await tc.sleepMs(1_000);

      // /agent create + edit: locally written/edited profiles would never
      // be discovered by the remote agent — both refuse.
      await typeLine(tc, '/agent create cloudpoke');
      await tc.waitForText('/agent create is not available in cloud sessions', 15_000);
      await tc.sleepMs(5_500);

      await typeLine(tc, '/agent edit localhero');
      await tc.waitForText('/agent edit is not available in cloud sessions', 15_000);
      await tc.sleepMs(5_500);

      // /workflow*: localOnly commands are dispatcher-refused in cloud
      // (kiro-agent #178) — silently, so the contract is the ABSENCE of any
      // recipe output or error after the submit. The seeded recipe name is
      // the leak probe; the run command references it by file stem, so any
      // launch path (monitor row, recipe echo, error) trips the probe. The
      // command echo row is excluded the same way the /spec leak probe
      // excludes it — `/workflow` as a command token.
      await typeLine(tc, '/workflow run LOCALFLOW_RECIPE_PROBE');
      await tc.sleepMs(3_000);
      snapshot = tc.getSnapshotFormatted();
      const workflowLeaks = snapshot
        .split('\n')
        .filter(
          (row) =>
            row.includes('LOCALFLOW_RECIPE_PROBE') &&
            !/\/workflow(\s|$)/.test(row)
        );
      expect(workflowLeaks).toEqual([]);
      expect(snapshot).not.toContain('Internal error');
      expect(snapshot).not.toContain('rejected by sandbox');
    },
    240_000
  );

  it.skipIf(skip)(
    '/mcp panel: seeded workspace mcp.json server never renders as sandbox state',
    async () => {
      const ws = seedConfigWorkspace();
      tempDirs.push(ws);
      harness = await CloudHarness.launch({
        testName: 'cloud-prefed-mcp',
        cwd: ws,
      });
      const tc = harness.testCase!;
      await tc.waitForText('Cloud session created', BOOT_TIMEOUT);
      await tc.waitForText('ask a question', 20_000);

      // The existing panel test (cloud-panels.test.ts) pins the awaiting
      // notice on an unseeded workspace; this variant proves the stronger
      // claim — with a REAL local mcp.json present, the panel still shows
      // sandbox provenance and never the local server row.
      await typeLine(tc, '/mcp');
      await tc.waitForText('MCP', 15_000);
      await tc.sleepMs(2_000);
      const snapshot = tc.getSnapshotFormatted();
      expect(snapshot).not.toContain('LOCALMCP_SERVER_PROBE');
      expect(snapshot).not.toContain('Internal error');
      await tc.pressEscape();
    },
    180_000
  );

  it.skipIf(skip)(
    'local control: the seeded workflow recipe IS discoverable without --cloud',
    async () => {
      // The positive control for test 1's /workflow no-leak probe: the KAS
      // recipe loader lists `<name>.workflow.json` files by FILENAME, so
      // the same seed must appear in the local recipe picker. Without this
      // leg, the cloud absence assertion could pass because the seed was
      // never loadable at all.
      const ws = seedConfigWorkspace();
      tempDirs.push(ws);
      harness = await CloudHarness.launch({
        testName: 'cloud-prefed-workflow-local',
        cwd: ws,
        cliArgs: [],
        ...WORKFLOWS_OPT_IN,
      });
      const tc = harness.testCase!;
      await tc.waitForText('ask a question', BOOT_TIMEOUT);

      // Bare `/workflow run` opens the recipe picker over the loader's
      // listing — the seeded recipe must be a row.
      await typeLine(tc, '/workflow run');
      await tc.waitForText('LOCALFLOW_RECIPE_PROBE', 15_000);
      expect(tc.getSnapshotFormatted()).not.toContain('Internal error');
      await tc.pressEscape();
    },
    120_000
  );
});
