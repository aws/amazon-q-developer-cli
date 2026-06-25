#!/usr/bin/env bun
import { spawn, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { readFileSync, existsSync } from 'node:fs';
import { resolveChatCliBin } from '../src/utils/chat-cli-bin';

const REPO_ROOT = resolve(import.meta.dir, '../../..');
const CARGO_BIN = resolveChatCliBin();
const TWINKI_DIR = resolve(REPO_ROOT, 'packages/twinki/packages/twinki');

// Resolve the pinned bun binary (matches the version shipped in the release binary)
function getPinnedBun(): string {
  // Read pinned version from const.py
  const constPy = resolve(REPO_ROOT, 'scripts/const.py');
  const versionMatch = readFileSync(constPy, 'utf8').match(
    /^BUN_VERSION\s*=\s*"(.+)"/m
  );
  const pinnedVersion = versionMatch?.[1] ?? 'unknown';
  console.log(`Pinned bun version: ${pinnedVersion}`);

  // Check if already cached
  const cachedBin = resolve(REPO_ROOT, `.bun-pinned/${pinnedVersion}/bun`);
  const isCached = existsSync(cachedBin);
  console.log(
    isCached
      ? `Using cached bun at ${cachedBin}`
      : `Downloading bun v${pinnedVersion}...`
  );

  const result = spawnSync(
    'bash',
    [resolve(REPO_ROOT, 'scripts/ensure-pinned-bun.sh')],
    {
      stdio: ['inherit', 'pipe', 'inherit'],
    }
  );
  if (result.status !== 0) {
    console.error('Failed to resolve pinned bun. Falling back to system bun.');
    return 'bun';
  }
  return result.stdout.toString().trim();
}

const PINNED_BUN = getPinnedBun();

// Separate dev-script flags from flags to forward to the TUI
const devFlags = new Set(['--skip-rust-build', '--local-kas']);
const skipRustBuild = process.argv.includes('--skip-rust-build');

// Optional: --local-kas uses a local kiro-agent checkout at local/kiro-agent/
// Skips CodeArtifact login and bun install, sets KIRO_KAS_SERVER_PATH.
// Auto-clones and builds kiro-agent if not present.
const localKas = process.argv.includes('--local-kas');
if (localKas) {
  const kasRoot = resolve(REPO_ROOT, 'local/kiro-agent');
  const serverPath = resolve(
    kasRoot,
    'packages/kiro-agent/dist/server/acp-server.js'
  );

  if (!existsSync(serverPath)) {
    if (!existsSync(kasRoot)) {
      console.log('Cloning kiro-agent into local/kiro-agent...');
      const clone = spawnSync(
        'git',
        ['clone', 'https://github.com/kiro-team/kiro-agent.git', kasRoot],
        {
          stdio: 'inherit',
        }
      );
      if (clone.status !== 0) {
        console.error(
          '❌ Failed to clone kiro-agent. Check your GitHub access.'
        );
        process.exit(1);
      }
    }

    console.log('Building kiro-agent...');
    const install = spawnSync('npm', ['install'], {
      cwd: kasRoot,
      stdio: 'inherit',
    });
    if (install.status !== 0) {
      console.error('❌ npm install failed in local/kiro-agent');
      process.exit(1);
    }
    const build = spawnSync('npm', ['run', 'build'], {
      cwd: kasRoot,
      stdio: 'inherit',
    });
    if (build.status !== 0) {
      console.error('❌ npm run build failed in local/kiro-agent');
      process.exit(1);
    }

    if (!existsSync(serverPath)) {
      console.error(
        `❌ Build completed but server not found at: ${serverPath}`
      );
      process.exit(1);
    }
  }

  process.env.KIRO_KAS_SERVER_PATH = serverPath;
  process.env.KIRO_AGENT_ENGINE = 'kas';
  console.log(`Using local KAS: ${serverPath}`);
}

// Optional: --rust-bin-path <path> overrides the default chat_cli binary
// and implicitly skips the Rust build (the path must already exist).
let rustBinOverride: string | null = null;
{
  const idx = process.argv.indexOf('--rust-bin-path');
  if (idx !== -1) {
    const value = process.argv[idx + 1];
    if (!value || value.startsWith('--')) {
      console.error('--rust-bin-path requires a path argument');
      process.exit(1);
    }
    rustBinOverride = resolve(process.cwd(), value);
    if (!existsSync(rustBinOverride)) {
      console.error(`--rust-bin-path: file not found: ${rustBinOverride}`);
      process.exit(1);
    }
  }
}

const RUST_BIN = rustBinOverride ?? CARGO_BIN;

// Everything after "dev" that isn't a dev-script flag gets forwarded to the TUI
const tuiArgs = process.argv.slice(2).filter((arg, i, arr) => {
  if (devFlags.has(arg)) return false;
  if (arg === '--rust-bin-path') return false;
  if (i > 0 && arr[i - 1] === '--rust-bin-path') return false;
  return true;
});

function buildTwinki(): boolean {
  console.log('Building twinki...');
  const result = spawnSync(
    'bunx',
    ['tsc', '--project', 'tsconfig.build.json'],
    {
      cwd: TWINKI_DIR,
      stdio: 'inherit',
    }
  );
  return result.status === 0;
}

function startTUI() {
  if (!existsSync(RUST_BIN)) {
    console.error(`\nError: Rust binary not found at ${RUST_BIN}`);
    console.error(`Run one of:`);
    console.error(`  cargo build -p chat_cli --bin chat_cli`);
    console.error(`  bun run dev  (without --skip-rust-build)`);
    console.error(`  bun run dev --rust-bin-path <path>\n`);
    process.exit(1);
  }

  console.log('Starting TUI...');

  // Start bun and forward any extra CLI args (e.g. --agent <name>) to the TUI.
  // Use absolute path to entry file so the caller's cwd is preserved.
  //
  // Deliberately NOT using --watch: a watch restart kills only the TUI process
  // and reparents chat_cli's child tree (aim mcp, otelcol-contrib) to PID 1,
  // leaking ~30MB per orphan and leaving zombie chat_cli processes. Restart the
  // dev server manually instead. (See commit 414f62298.)
  const entryFile = resolve(import.meta.dir, '../src/index.tsx');
  const bunProcess = spawn(PINNED_BUN, [entryFile, ...tuiArgs], {
    stdio: 'inherit',
    env: {
      ...process.env,
      // KAS in `--auth=acp-callback` mode shells out to this binary for
      // `chat _ get-kas-token` (host-mediated OIDC refresh). Also used by
      // V2's ACP child spawn and /chat save/load. Must be set regardless
      // of the active engine.
      KIRO_CHAT_CLI_BIN: RUST_BIN,
      JSC_numberOfGCMarkers: '1',
      // Dev mirrors a lite-cohort user so a persisted chat.ui.mode='lite' is
      // honored (resolveUiMode gates on this; prod gets it from launch.rs).
      KIRO_LITE_ROLLOUT_ENABLED: process.env.KIRO_LITE_ROLLOUT_ENABLED ?? '1',
    },
  });

  bunProcess.on('exit', (code) => {
    process.exit(code ?? 0);
  });
}

// Verify CodeArtifact auth is valid and refresh if expired
if (localKas) {
  console.log('Skipping CodeArtifact login (--local-kas)...');
} else {
  const npmrc = resolve(REPO_ROOT, '.npmrc');
  let needsLogin =
    !existsSync(npmrc) ||
    !readFileSync(npmrc, 'utf8').includes('@kiro:registry');
  if (!needsLogin) {
    const tokenMatch = readFileSync(npmrc, 'utf8').match(/:_authToken=(.+)/);
    if (tokenMatch) {
      try {
        const header = JSON.parse(
          Buffer.from(tokenMatch[1]!.split('.')[0]!, 'base64url').toString()
        );
        if (header.exp && header.exp < Date.now() / 1000) needsLogin = true;
      } catch {
        needsLogin = true;
      }
    } else {
      needsLogin = true;
    }
  }
  if (needsLogin) {
    console.log('CodeArtifact token missing or expired, refreshing...');
    const login = spawnSync(
      'bash',
      [resolve(REPO_ROOT, 'scripts/codeartifact-login.sh')],
      {
        cwd: REPO_ROOT,
        stdio: 'inherit',
      }
    );
    if (login.status !== 0) {
      console.error(
        'CodeArtifact login failed. Run manually: ./scripts/codeartifact-login.sh'
      );
      process.exit(1);
    }
  }
}

// Ensure dependencies are installed (<50ms when no deps changed)
if (!localKas) {
  spawnSync('bun', ['install'], { cwd: REPO_ROOT, stdio: 'inherit' });
}

if (skipRustBuild || rustBinOverride) {
  console.log(
    rustBinOverride
      ? `Using Rust binary at ${RUST_BIN}`
      : 'Skipping Rust build...'
  );
  if (!buildTwinki()) {
    console.error('Twinki build failed');
    process.exit(1);
  }
  startTUI();
} else {
  console.log('Building chat_cli...');

  // Build the Rust binary
  const cargoBuild = spawn(
    'cargo',
    ['build', '-p', 'chat_cli', '--bin', 'chat_cli'],
    {
      cwd: REPO_ROOT,
      stdio: 'inherit',
    }
  );

  cargoBuild.on('exit', (code) => {
    if (code !== 0) {
      console.error('Cargo build failed');
      process.exit(code ?? 1);
    }

    console.log('Generating TypeScript types...');

    // Generate types
    const typeGen = spawn('./scripts/generate-types.sh', [], {
      cwd: REPO_ROOT,
      stdio: 'inherit',
    });

    typeGen.on('exit', (code) => {
      if (code !== 0) {
        console.error('Type generation failed');
        process.exit(code ?? 1);
      }

      if (!buildTwinki()) {
        console.error('Twinki build failed');
        process.exit(1);
      }

      startTUI();
    });
  });
}
