#!/usr/bin/env bun
/**
 * Launch the debug TUI against a real backend in a private tmux session,
 * with every V2 and KAS session fixture seeded into a sandbox KIRO_HOME.
 *
 * Use it to interactively verify cross-engine resume / conversion: pick a
 * fixture, resume it, and scroll the rendered history. The TUI runs from
 * source (picks up local TS changes); the chat_cli binary and the
 * kiro-agent server are external inputs.
 *
 * Prerequisites:
 *   - Debug chat_cli:  cargo build -p chat_cli --bin chat_cli
 *   - KAS engine: `@kiro/agent` installed (bun install) or --kas-server <path>
 *
 * Usage:
 *   bun run scripts/test-convert-e2e.ts [options]
 *
 * Options:
 *   --engine kas|v2     Backend engine (default: kas)
 *   --resume <name>     Launch resuming a fixture by name (e.g. with_compaction)
 *   --kas-server <path> Override the kiro-agent server.js (default: installed @kiro/agent)
 *   --session <name>    tmux session name (default: kiro-convert-e2e)
 *   --keep              Keep the sandbox KIRO_HOME on exit (default: removed via trap note)
 *   --list              List available fixtures and exit
 */
import { spawnSync } from 'node:child_process';
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readdirSync,
  readFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { requireChatCliBin } from '../src/utils/chat-cli-bin';

const REPO_ROOT = resolve(import.meta.dir, '../../..');
const TUI_DIR = resolve(import.meta.dir, '..');
const CHAT_CLI = requireChatCliBin();
const FIXTURES_ROOT = resolve(
  REPO_ROOT,
  'crates/chat-cli-v2/src/agent/kas/fixtures'
);

const argv = process.argv.slice(2);
function flag(name: string, fallback?: string): string | undefined {
  const i = argv.indexOf(name);
  return i !== -1 && argv[i + 1] && !argv[i + 1]!.startsWith('--')
    ? argv[i + 1]
    : fallback;
}
const has = (name: string) => argv.includes(name);

const engine = flag('--engine', 'kas')!;
const resumeName = flag('--resume');
const kasServer = flag('--kas-server');
const sessionName = flag('--session', 'kiro-convert-e2e')!;
const keep = has('--keep');

/** A seedable session fixture: a `<dir>/session.json` + `messages.jsonl` pair. */
interface Fixture {
  name: string;
  format: 'v2' | 'kas';
  dir: string;
  sessionId: string;
}

/** Discover every fixture under fixtures/{v2,kas} that has a session.json. */
function discoverFixtures(): Fixture[] {
  const out: Fixture[] = [];
  for (const format of ['v2', 'kas'] as const) {
    const base = join(FIXTURES_ROOT, format);
    if (!existsSync(base)) continue;
    for (const name of readdirSync(base)) {
      const dir = join(base, name);
      const meta = join(dir, 'session.json');
      if (!existsSync(meta)) continue;
      const parsed = JSON.parse(readFileSync(meta, 'utf8'));
      const sessionId = parsed.session_id ?? parsed.id;
      if (sessionId) out.push({ name, format, dir, sessionId });
    }
  }
  return out;
}

/** Seed a fixture into the sandbox under the layout its engine expects. */
function seed(home: string, fx: Fixture): void {
  if (fx.format === 'v2') {
    const dir = join(home, 'sessions', 'cli');
    mkdirSync(dir, { recursive: true });
    copyFileSync(join(fx.dir, 'session.json'), join(dir, `${fx.sessionId}.json`));
    copyFileSync(
      join(fx.dir, 'messages.jsonl'),
      join(dir, `${fx.sessionId}.jsonl`)
    );
  }
  // KAS fixtures carry their own workspace-hash bucket layout; copy the
  // whole directory tree under sessions/ verbatim when present.
  else {
    const dest = join(home, 'sessions');
    mkdirSync(dest, { recursive: true });
    spawnSync('cp', ['-R', join(fx.dir, '.'), dest]);
  }
}

const fixtures = discoverFixtures();

if (has('--list')) {
  console.log('Available fixtures:');
  for (const f of fixtures) {
    console.log(`  ${f.format.padEnd(4)} ${f.name}  (${f.sessionId})`);
  }
  process.exit(0);
}

if (!existsSync(CHAT_CLI)) {
  console.error(`chat_cli not found at ${CHAT_CLI}`);
  console.error('Build it first: cargo build -p chat_cli --bin chat_cli');
  process.exit(1);
}

if (resumeName && !fixtures.some((f) => f.name === resumeName)) {
  console.error(`Unknown fixture "${resumeName}". Run with --list to see options.`);
  process.exit(1);
}

// Sandbox KIRO_HOME with every fixture seeded.
const home = mkdtempSync(join(tmpdir(), 'kiro-convert-e2e-'));
for (const fx of fixtures) seed(home, fx);

const env: Record<string, string> = {
  KIRO_HOME: home,
  KIRO_CHAT_CLI_BIN: CHAT_CLI,
  KIRO_FEED_FILE: resolve(REPO_ROOT, 'crates/chat-cli-v2/src/cli/feed.json'),
};
if (engine === 'kas') {
  env.KIRO_AGENT_ENGINE = 'kas';
  // KAS runs as a node child; the TUI resolves the installed @kiro/agent
  // unless KIRO_KAS_SERVER_PATH overrides it.
  env.KIRO_AGENT_PATH = 'node';
  if (kasServer) env.KIRO_KAS_SERVER_PATH = resolve(process.cwd(), kasServer);
} else {
  env.KIRO_AGENT_PATH = CHAT_CLI;
}

const resumeFixture = fixtures.find((f) => f.name === resumeName);
const tuiArgs = ['./src/index.tsx'];
if (resumeFixture) tuiArgs.push('--resume-id', resumeFixture.sessionId);

// Private tmux socket (tmux-skill convention) so this never touches the
// user's default tmux server.
const socketDir = join(tmpdir(), 'agent-tmux-sockets');
mkdirSync(socketDir, { recursive: true });
const socket = join(socketDir, 'kiro-convert-e2e.sock');
const tmux = (args: string[]) =>
  spawnSync('tmux', ['-S', socket, ...args], { stdio: 'inherit' });

tmux(['kill-session', '-t', sessionName]); // best-effort reset

const envPrefix = Object.entries(env)
  .map(([k, v]) => `${k}=${JSON.stringify(v)}`)
  .join(' ');
const launch = `${envPrefix} bun ${tuiArgs.map((a) => JSON.stringify(a)).join(' ')}`;

const created = tmux([
  '-f',
  '/dev/null',
  'new',
  '-d',
  '-s',
  sessionName,
  '-n',
  'tui',
  '-x',
  '160',
  '-y',
  '48',
  '-c',
  TUI_DIR,
  launch,
]);
if (created.status !== 0) {
  console.error('Failed to start tmux session.');
  process.exit(1);
}

console.log('');
console.log(`Engine:    ${engine}`);
console.log(`KIRO_HOME: ${home}`);
console.log(`Fixtures:  ${fixtures.map((f) => f.name).join(', ') || '(none)'}`);
if (resumeFixture)
  console.log(`Resuming:  ${resumeFixture.name} (${resumeFixture.sessionId})`);
console.log('');
console.log('Monitor:   ' + `tmux -S ${socket} attach -t ${sessionName}`);
console.log(
  'Capture:   ' +
    `tmux -S ${socket} capture-pane -p -J -t ${sessionName}:0.0 -S -200`
);
console.log('Stop:      ' + `tmux -S ${socket} kill-session -t ${sessionName}`);
console.log(
  keep
    ? `Sandbox kept at ${home}`
    : `Sandbox at ${home} (remove with: rm -rf ${home})`
);
