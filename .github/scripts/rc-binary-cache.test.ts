import { describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

import {
  ASSET_ENV_CONTRACT_PATH,
  FORBIDDEN_EMBEDDING_ENV,
  SOURCE_PATHS,
  buildCommand,
  buildPlan,
  cacheEnabled,
  classifyRestore,
  createBinaryIdentity,
  findForbiddenEnvironment,
  firstDifferencePath,
  hashSourceIndex,
  hashTrackedFiles,
  identityDigest,
  prepareBundle,
  restoreBinary,
  stableToolOutput,
  stableStringify,
  stageBinary,
  verifyBundle,
  type BinaryIdentity,
  type BinaryIdentityInput,
} from './rc-binary-cache';

function identityInput(): BinaryIdentityInput {
  return {
    source: {
      contentSha256: 'c'.repeat(64),
      fileCount: 3000,
      gitObjectFormat: 'sha1',
      indexSha256: 'a'.repeat(64),
      paths: SOURCE_PATHS,
    },
    runner: {
      label: 'ubuntu-latest-8-cores',
      os: 'Linux',
      arch: 'X64',
      imageOS: 'ubuntu24',
      imageVersion: '20260810.1',
      system: 'Ubuntu 24.04',
    },
    toolchain: {
      rustc: 'rustc 1.92.0',
      cargo: 'cargo 1.92.0',
      native: {
        alsa: '1.2.11',
        alsaPackage: 'libasound2-dev:amd64|1.2.11-1ubuntu0.1|amd64',
        cc: 'gcc 14',
        ld: 'GNU ld 2.42',
      },
      nativeComplete: true,
    },
    build: {
      program: 'cargo',
      args: ['build', '-p', 'chat_cli', '--release', '--locked'],
      package: 'chat_cli',
      profile: 'release',
      binaryName: 'chat_cli',
      binaryPath: 'target/release/chat_cli',
    },
    environment: {
      AMAZON_Q_BUILD_DATETIME: null,
      AMAZON_Q_BUILD_HASH: null,
      KIRO_VERSION: null,
      RUSTFLAGS: null,
      RUSTC_WRAPPER: 'sccache',
    },
  };
}

function identity(): BinaryIdentity {
  return createBinaryIdentity(identityInput());
}

function withTempDirectory(run: (directory: string) => void): void {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'rc-binary-cache-'));
  try {
    run(directory);
  } finally {
    fs.rmSync(directory, { force: true, recursive: true });
  }
}

const CERTIFICATION_WORKFLOW = '.github/workflows/rc-certification.yml';
const FORK_WORKFLOW = '.github/workflows/fork-ci.yml';
const CODEARTIFACT_LOGIN = '.github/actions/codeartifact-login/action.yml';

const CERTIFICATION_JOBS = [
  'resolve',
  'tui-static',
  'twinki-perf',
  'build-assets',
  'ui-integration',
  'acp-integration',
  'e2e',
  'smoke',
  'scenario-categories',
  'visual',
  'live-parity',
  'summary',
  'tui-unit-tests-required',
  'tui-typecheck-required',
  'tui-lint-required',
  'tui-integ-tests-required',
  'tui-e2e-required',
];

// Fork runs execute from the untrusted checkout, so shape assertions must read
// the same trusted copy of the workflow that GitHub actually ran.
function trustedCiRoot(): string {
  return process.env.RC_CI_ROOT || process.cwd();
}

function readCertificationWorkflow(): string {
  return fs.readFileSync(
    path.join(trustedCiRoot(), CERTIFICATION_WORKFLOW),
    'utf8'
  );
}

function readForkWorkflow(): string {
  return fs.readFileSync(path.join(trustedCiRoot(), FORK_WORKFLOW), 'utf8');
}

// A local `uses: ./…` step runs whatever that definition itself pulls in, so a
// pin assertion is only complete if it follows those references too.
function localDefinitions(workflow: string): string[] {
  const references = new Set(
    [...workflow.matchAll(/^ *(?:- )?uses: \.\/(\S+)/gm)].map((match) =>
      (match[1] as string).replace(/^\.trusted-ci\//, '')
    )
  );

  return [...references].map((reference) =>
    fs.readFileSync(
      path.join(
        trustedCiRoot(),
        /\.ya?ml$/.test(reference)
          ? reference
          : path.join(reference, 'action.yml')
      ),
      'utf8'
    )
  );
}

function certificationJobs(workflow: string): Map<string, string> {
  const section = workflow.slice(workflow.indexOf('\njobs:\n'));
  const boundaries = [...section.matchAll(/\n {2}([a-z][a-z0-9-]*):\n/g)];
  const jobs = new Map<string, string>();
  boundaries.forEach((boundary, position) => {
    const start = boundary.index ?? 0;
    const end = boundaries[position + 1]?.index ?? section.length;
    jobs.set(boundary[1] as string, section.slice(start, end));
  });
  return jobs;
}

function steps(job: string, name: string): string[] {
  return job
    .split(`- name: ${name}`)
    .slice(1)
    .map((rest) => {
      const next = rest.indexOf('\n      - ');
      return next === -1 ? rest : rest.slice(0, next);
    });
}

describe('RC binary cache identity', () => {
  it('gives automatic certification lanes three total attempts', () => {
    const workflow = readCertificationWorkflow();

    expect(workflow).toContain(
      "lane_attempts:\n        description: Full lane attempts for flaky workflow steps\n        type: choice\n        default: '3'"
    );
    expect(workflow.match(/echo "lane_attempts=3"/g)).toHaveLength(4);
    expect(workflow).not.toContain('echo "lane_attempts=2"');
  });

  it('is canonical regardless of environment insertion order', () => {
    const left = identityInput();
    const right = identityInput();
    right.environment = {
      RUSTC_WRAPPER: 'sccache',
      RUSTFLAGS: null,
      KIRO_VERSION: null,
      AMAZON_Q_BUILD_HASH: null,
      AMAZON_Q_BUILD_DATETIME: null,
    };

    expect(identityDigest(createBinaryIdentity(left))).toBe(
      identityDigest(createBinaryIdentity(right))
    );
  });

  it('changes for source, runner, toolchain, build, and environment inputs', () => {
    const baseline = identityDigest(identity());
    const variants = [
      () => {
        const value = identityInput();
        value.source.indexSha256 = 'b'.repeat(64);
        return value;
      },
      () => {
        const value = identityInput();
        value.runner.imageVersion = '20260811.1';
        return value;
      },
      () => {
        const value = identityInput();
        value.toolchain.rustc = 'rustc 1.93.0';
        return value;
      },
      () => {
        const value = identityInput();
        value.toolchain.native.alsaPackage =
          'libasound2-dev:amd64|1.2.11-1ubuntu0.2|amd64';
        return value;
      },
      () => {
        const value = identityInput();
        value.build.args.push('--no-default-features');
        return value;
      },
      () => {
        const value = identityInput();
        value.environment.RUSTFLAGS = '-C target-cpu=native';
        return value;
      },
    ];

    for (const variant of variants) {
      expect(identityDigest(createBinaryIdentity(variant()))).not.toBe(
        baseline
      );
    }
  });

  it('reports identity drift without exposing field values', () => {
    const expected = identity();
    const current = structuredClone(expected);
    current.runner.imageVersion = 'different-version';

    expect(firstDifferencePath(expected, current)).toBe(
      'identity.runner.imageVersion'
    );
    expect(firstDifferencePath(expected, structuredClone(expected))).toBe(
      undefined
    );
  });

  it('owns platform build execution in one typed plan', () => {
    const linux = buildPlan('Linux');
    const windows = buildPlan('Windows');

    expect(buildCommand(linux)).toEqual([
      'cargo',
      'build',
      '-p',
      'chat_cli',
      '--release',
      '--locked',
    ]);
    expect(buildCommand(windows)).toEqual([
      'cargo',
      'build',
      '-p',
      'chat_cli',
      '--release',
      '--no-default-features',
      '--locked',
    ]);
    expect(windows.binaryPath).toBe('target/release/chat_cli.exe');

    const workflow = readCertificationWorkflow();
    expect(workflow).toMatch(/rc-binary-cache\.ts"? build --identity-path/);
    expect(workflow).not.toContain('cargo build -p chat_cli');

    const consumers = workflow
      .split('\n      - name: Download reusable bundle')
      .slice(1);
    expect(consumers).toHaveLength(7);
    expect(workflow).not.toContain('Restore binary mode');
    for (const consumer of consumers) {
      const verify = consumer.indexOf('\n      - name: Verify reusable binary');
      const execute = consumer.search(/\n      - name: Run /);
      expect(verify).toBeGreaterThan(-1);
      expect(execute).toBeGreaterThan(verify);
    }
  });

  it('keeps fork cache access read-only and validates before checkout', () => {
    const workflow = readCertificationWorkflow();
    const jobs = certificationJobs(workflow);
    expect([...jobs.keys()]).toEqual(CERTIFICATION_JOBS);

    const untrusted = [...jobs].filter(([, job]) =>
      job.includes('ref: ${{ inputs.source_sha')
    );
    expect(untrusted.map(([name]) => name)).toHaveLength(12);
    expect(
      untrusted
        .filter(([, job]) => {
          const validate = job.indexOf('- name: Validate invocation');
          if (validate > -1 && validate < job.indexOf('ref: ${{ inputs.')) {
            return false;
          }
          // Needing resolve is equivalent, because resolve validates first —
          // but only for a job that cannot run once resolve has failed.
          const header = job.slice(0, job.indexOf('\n    steps:'));
          return (
            !/\n {4}needs:.*\bresolve\b/.test(header) ||
            header.includes('always()')
          );
        })
        .map(([name]) => name)
    ).toEqual([]);

    expect(workflow).toContain(
      "EXECUTION_MODE: ${{ inputs.execution_mode || 'internal' }}"
    );
    expect(workflow).toContain(
      "- name: Restore cargo registry\n        if: inputs.execution_mode == 'fork'"
    );
    expect(workflow).toContain(
      "- name: Cache cargo registry\n        if: inputs.execution_mode != 'fork'"
    );
    expect(workflow).toContain(
      "- name: Save exact Rust binary cache\n        if: inputs.execution_mode != 'fork'"
    );
    expect(workflow).toContain(
      "- name: Configure sccache\n        if: inputs.execution_mode != 'fork'"
    );
  });

  it('takes trusted CI support from the calling base commit', () => {
    const workflow = readCertificationWorkflow();

    expect(workflow).not.toContain('ref: ${{ inputs.trusted_sha }}');
    expect(workflow).toContain('TRUSTED_SHA: ${{ inputs.trusted_sha }}');
    expect(workflow).toContain('TRUSTED_BASE_SHA: ${{ github.sha }}');
    expect(workflow).toContain('! "$TRUSTED_SHA" =~ ^[0-9a-f]{40}$');
    expect(workflow).toContain('"$TRUSTED_SHA" != "$TRUSTED_BASE_SHA"');

    const trusted = steps(workflow, 'Checkout trusted CI support');
    expect(trusted).toHaveLength(12);
    for (const checkout of trusted) {
      expect(checkout).toContain("if: inputs.execution_mode == 'fork'");
      expect(checkout).toContain('repository: ${{ github.repository }}');
      expect(checkout).toContain('ref: ${{ github.sha }}');
      expect(checkout).toContain('persist-credentials: false');
      expect(checkout).toContain('scripts/const.py');
    }
  });

  it('reads the pinned Bun version from the trusted checkout', () => {
    const workflow = readCertificationWorkflow();
    const bunVersion = steps(workflow, 'Read pinned Bun version');

    expect(bunVersion).toHaveLength(12);
    for (const step of bunVersion) {
      expect(step).toContain('shell: bash');
      expect(step).toContain('"$RC_CI_ROOT/scripts/const.py"');
      expect(step).not.toMatch(/[^/]scripts\/const\.py/);
      expect(step).toContain('^[0-9]+\\.[0-9]+\\.[0-9]+$');
    }
  });

  it('pins every third-party action a fork run reaches to a commit SHA', () => {
    const workflow = readCertificationWorkflow();
    const local = localDefinitions(workflow);
    expect(local.length).toBeGreaterThan(0);

    const external = [workflow, ...local].flatMap((definition) =>
      [...definition.matchAll(/^ *(?:- )?uses: (?!\.\/)(\S+)/gm)].map(
        (match) => match[1] as string
      )
    );

    expect(external.length).toBeGreaterThan(0);
    expect(
      external.filter((ref) => !/^[\w.-]+\/[\w./-]+@[0-9a-f]{40}$/.test(ref))
    ).toEqual([]);
  });

  it('keeps the registry credential out of logs and out of later steps', () => {
    const login = fs.readFileSync(
      path.join(trustedCiRoot(), CODEARTIFACT_LOGIN),
      'utf8'
    );

    expect(login).toContain('echo "::add-mask::$TOKEN"');
    expect(login.indexOf('::add-mask::')).toBeLessThan(
      login.indexOf('> .npmrc')
    );
    for (const variable of [
      'AWS_ACCESS_KEY_ID=',
      'AWS_SECRET_ACCESS_KEY=',
      'AWS_SESSION_TOKEN=',
    ]) {
      expect(login).toContain(`echo '${variable}'`);
    }
  });

  it('grants no job more than it needs', () => {
    const workflow = readCertificationWorkflow();
    const jobs = certificationJobs(workflow);
    const header = workflow.slice(0, workflow.indexOf('\njobs:\n'));

    expect(header).toContain('permissions:\n  contents: read\n');
    expect(header).not.toContain('id-token: write');
    expect(header).not.toContain('pull-requests:');
    expect(
      [...jobs]
        .filter(([, job]) => !job.includes('\n    permissions:'))
        .map(([name]) => name)
    ).toEqual([]);
    expect(
      [...jobs]
        .filter(([, job]) =>
          /\n {6}(?:contents|pull-requests): write/.test(job)
        )
        .map(([name]) => name)
    ).toEqual([]);
    expect(
      [...jobs]
        .filter(([, job]) => job.includes('id-token: write'))
        .map(([name]) => name)
    ).toEqual([
      'tui-static',
      'twinki-perf',
      'build-assets',
      'ui-integration',
      'acp-integration',
      'e2e',
      'smoke',
      'scenario-categories',
      'visual',
      'live-parity',
    ]);
  });

  it('can inspect the protected fork approval environment', () => {
    const workflow = readForkWorkflow();
    const header = workflow.slice(0, workflow.indexOf('\njobs:\n'));

    expect(header).toContain('permissions:\n  actions: read\n');
    expect(header).not.toContain('deployments: read');
    expect(workflow).toContain('github.rest.repos.getEnvironment');
    expect(readCertificationWorkflow()).toContain(
      '.github/workflows/fork-ci.yml'
    );
  });

  it('keeps TUI-only checks independent from Rust asset builds', () => {
    const jobs = certificationJobs(readCertificationWorkflow());

    for (const name of ['tui-static', 'twinki-perf']) {
      const job = jobs.get(name) as string;
      const header = job.slice(0, job.indexOf('\n    steps:'));
      expect(header).toContain('\n    needs: resolve\n');
      expect(header).not.toContain('build-assets');
    }

    expect(jobs.get('twinki-perf')).toContain(
      "if: needs.resolve.outputs.run_twinki_perf == 'true'"
    );
    expect(jobs.get('tui-static')).toContain("FORCE_COLOR: '0'");
  });

  it('filters documentation without hiding required TUI statuses', () => {
    const workflow = readCertificationWorkflow();
    const jobs = certificationJobs(workflow);
    const pathFilter = steps(workflow, 'Detect TUI changes');
    const header = workflow.slice(0, workflow.indexOf('\njobs:\n'));

    expect(header).toContain('on:\n  pull_request:\n  schedule:');
    expect(pathFilter).toHaveLength(1);
    expect(pathFilter[0]).toContain('predicate-quantifier: every');
    expect(pathFilter[0]).toContain('infrastructure:');
    expect(pathFilter[0]).toContain(
      "'{package.json,bun.lock,eslint.config.js,.prettierrc,scripts/const.py,"
    );
    for (const exclusion of [
      "'!packages/tui/**/*.md'",
      "'!packages/tui/docs/**'",
      "'!packages/twinki/**/*.md'",
      "'!packages/twinki/docs/**'",
    ]) {
      expect(pathFilter[0]).toContain(exclusion);
    }
    expect(workflow).toContain(
      'run_twinki_perf: ${{ steps.resolve.outputs.run_twinki_perf }}'
    );

    for (const name of [
      'tui-unit-tests-required',
      'tui-typecheck-required',
      'tui-lint-required',
      'tui-integ-tests-required',
      'tui-e2e-required',
    ]) {
      expect(jobs.get(name)).toContain('\n    if: always()\n');
    }
  });

  it('uses only the selected stream for stable tool identity', () => {
    const stdout = [
      'rustc 1.92.0 (ded5c06cf 2025-12-08)',
      'commit-hash: ded5c06cf21d2b93bffd5d884aa6e96934ee4234',
      'host: x86_64-unknown-linux-gnu',
    ].join('\n');

    expect(stableToolOutput(stdout, '')).toBe(stdout);
    expect(stableToolOutput(stdout, 'info: transient rustup message')).toBe(
      stdout
    );
    expect(stableToolOutput('transient stdout', 'LLD 14.0.0', 'stderr')).toBe(
      'LLD 14.0.0'
    );
    expect(() => stableToolOutput('', 'warning: no version')).toThrow(
      'Tool produced no stdout'
    );
    expect(() => stableToolOutput('diagnostic', '', 'stderr')).toThrow(
      'Tool produced no stderr'
    );
  });

  it('hashes tracked Git entries rather than commit metadata', () => {
    const first = Buffer.from(
      '100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 0\tCargo.lock\0'
    );
    const second = Buffer.from(
      '100644 bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb 0\tCargo.lock\0'
    );

    expect(hashSourceIndex(first)).not.toBe(hashSourceIndex(second));
    expect(hashSourceIndex(first)).toBe(hashSourceIndex(Buffer.from(first)));
  });

  it('hashes the checked-out bytes as well as Git index entries', () => {
    withTempDirectory((directory) => {
      const filePath = path.join(directory, 'Cargo.lock');
      const index = Buffer.from(
        '100644 aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa 0\tCargo.lock\0'
      );
      fs.writeFileSync(filePath, 'first contents');
      const first = hashTrackedFiles(index, directory);
      fs.writeFileSync(filePath, 'second contents');

      expect(hashTrackedFiles(index, directory)).not.toBe(first);
    });
  });

  it('covers Rust, Cargo, embedded docs, workflow, and cache logic only', () => {
    expect(SOURCE_PATHS).toContain('crates/**');
    expect(SOURCE_PATHS).toContain('autodocs/**');
    expect(SOURCE_PATHS).toContain('autodocs-v2/**');
    expect(SOURCE_PATHS).toContain('.github/workflows/rc-certification.yml');
    expect(SOURCE_PATHS).toContain('.github/scripts/rc-binary-cache.ts');
    expect(SOURCE_PATHS).toContain('.github/scripts/rc-binary-cache/**');
    expect(SOURCE_PATHS.some((entry) => entry.startsWith('packages/tui'))).toBe(
      false
    );
  });

  it('rejects every asset-embedding environment variable, including empty values', () => {
    const environment = Object.fromEntries(
      FORBIDDEN_EMBEDDING_ENV.map((name) => [name, ''])
    );
    expect(findForbiddenEnvironment(environment)).toEqual([
      ...FORBIDDEN_EMBEDDING_ENV,
    ]);
    expect(findForbiddenEnvironment({ tui_js_path: '/tmp/tui.js' })).toEqual([
      'TUI_JS_PATH',
    ]);
  });

  it('constrains Rust asset environment references to the shared contract', () => {
    const contract = fs
      .readFileSync(ASSET_ENV_CONTRACT_PATH, 'utf8')
      .split(/\r?\n/)
      .map((line) => line.trim())
      .filter((line) => line.length > 0 && !line.startsWith('#'));
    expect(contract).toEqual([...FORBIDDEN_EMBEDDING_ENV]);

    const trackedRust = Bun.spawnSync({
      cmd: ['git', 'ls-files', '-z', '--', 'crates'],
      stdout: 'pipe',
      stderr: 'pipe',
    });
    expect(trackedRust.exitCode).toBe(0);

    const referenced = new Set<string>();
    const buildScriptAssetName =
      /\b(?:(?:BUN|TUI|NODE|KAS)_[A-Z0-9_]+|DISABLE_V2_BUN|INCLUDE_KAS_BUNDLE)\b/g;
    const compileTimeEnvironment =
      /(?:env|option_env)!\("([A-Z][A-Z0-9_]+)"\)/g;
    const assetName =
      /^(?:(?:BUN|TUI|NODE|KAS)_|DISABLE_V2_BUN|INCLUDE_KAS_BUNDLE)/;
    const rustFiles = Buffer.from(trackedRust.stdout)
      .toString('utf8')
      .split('\0')
      .filter((filePath) => filePath.endsWith('.rs'));

    for (const rustFile of rustFiles) {
      const contents = fs.readFileSync(rustFile, 'utf8');
      if (path.basename(rustFile) === 'build.rs') {
        for (const match of contents.matchAll(buildScriptAssetName)) {
          referenced.add(match[0]);
        }
      }
      for (const match of contents.matchAll(compileTimeEnvironment)) {
        const name = match[1];
        if (name !== undefined && assetName.test(name)) {
          referenced.add(name);
        }
      }
    }

    expect(referenced.size).toBeGreaterThan(0);
    expect(
      [...referenced].filter((name) => !contract.includes(name)).sort()
    ).toEqual([]);
  });

  it('uses only exact cache hits', () => {
    expect(classifyRestore(true, 'success', 'true')).toEqual({
      kind: 'restore',
      reason: 'exact-hit',
    });
    expect(classifyRestore(true, 'success', 'false')).toEqual({
      kind: 'rebuild',
      reason: 'cache-miss',
    });
    expect(classifyRestore(true, 'failure', '')).toEqual({
      kind: 'rebuild',
      reason: 'restore-error',
    });
    expect(classifyRestore(false, 'skipped', '')).toEqual({
      kind: 'rebuild',
      reason: 'cache-disabled',
    });
  });

  it('disables reuse when runner or native-tool identity is incomplete', () => {
    const complete = identity();
    expect(cacheEnabled(complete)).toBe(true);

    const missingImage = structuredClone(complete);
    missingImage.runner.imageVersion = 'unavailable';
    expect(cacheEnabled(missingImage)).toBe(false);

    const missingNativeTool = structuredClone(complete);
    missingNativeTool.toolchain.nativeComplete = false;
    expect(cacheEnabled(missingNativeTool)).toBe(false);
  });
});

describe('RC binary cache integrity', () => {
  it('stages and restores a valid binary and manifest', () => {
    withTempDirectory((directory) => {
      const source = path.join(directory, 'chat_cli');
      const cache = path.join(directory, 'cache');
      const destination = path.join(directory, 'target', 'chat_cli');
      const stagedManifest = path.join(directory, 'staged-manifest.json');
      const restoredManifest = path.join(directory, 'restored-manifest.json');
      fs.writeFileSync(source, 'known binary bytes');

      const manifest = stageBinary(identity(), source, cache, stagedManifest);
      const restored = restoreBinary(
        identity(),
        cache,
        destination,
        restoredManifest
      );

      expect(fs.readFileSync(destination, 'utf8')).toBe('known binary bytes');
      expect(restored.binary.sha256).toBe(manifest.binary.sha256);
      expect(fs.readFileSync(restoredManifest, 'utf8')).toBe(
        fs.readFileSync(stagedManifest, 'utf8')
      );
    });
  });

  it('rejects a corrupt binary without touching the destination', () => {
    withTempDirectory((directory) => {
      const source = path.join(directory, 'chat_cli');
      const cache = path.join(directory, 'cache');
      const destination = path.join(directory, 'target', 'chat_cli');
      const bundleManifest = path.join(directory, 'manifest.json');
      fs.mkdirSync(path.dirname(destination), { recursive: true });
      fs.writeFileSync(source, 'known binary bytes');
      fs.writeFileSync(destination, 'existing destination');
      stageBinary(identity(), source, cache, bundleManifest);
      fs.appendFileSync(path.join(cache, 'chat_cli'), 'corruption');

      expect(() =>
        restoreBinary(identity(), cache, destination, bundleManifest)
      ).toThrow('size does not match');
      expect(fs.readFileSync(destination, 'utf8')).toBe('existing destination');
    });
  });

  it('rejects an identity mismatch without touching the destination', () => {
    withTempDirectory((directory) => {
      const source = path.join(directory, 'chat_cli');
      const cache = path.join(directory, 'cache');
      const destination = path.join(directory, 'target', 'chat_cli');
      const bundleManifest = path.join(directory, 'manifest.json');
      fs.mkdirSync(path.dirname(destination), { recursive: true });
      fs.writeFileSync(source, 'known binary bytes');
      fs.writeFileSync(destination, 'existing destination');
      stageBinary(identity(), source, cache, bundleManifest);

      const changedInput = identityInput();
      changedInput.source.indexSha256 = 'b'.repeat(64);
      expect(() =>
        restoreBinary(
          createBinaryIdentity(changedInput),
          cache,
          destination,
          bundleManifest
        )
      ).toThrow('identity digest does not match');
      expect(fs.readFileSync(destination, 'utf8')).toBe('existing destination');
    });
  });

  it('rejects path traversal and unexpected cache files', () => {
    withTempDirectory((directory) => {
      const source = path.join(directory, 'chat_cli');
      const cache = path.join(directory, 'cache');
      const destination = path.join(directory, 'target', 'chat_cli');
      const bundleManifest = path.join(directory, 'manifest.json');
      fs.writeFileSync(source, 'known binary bytes');
      stageBinary(identity(), source, cache, bundleManifest);

      const manifestPath = path.join(cache, 'manifest.json');
      const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
      manifest.binary.name = '../chat_cli';
      fs.writeFileSync(manifestPath, `${stableStringify(manifest)}\n`);
      expect(() =>
        restoreBinary(identity(), cache, destination, bundleManifest)
      ).toThrow('binary name is invalid');

      stageBinary(identity(), source, cache, bundleManifest);
      fs.writeFileSync(path.join(cache, 'unexpected'), 'unexpected');
      expect(() =>
        restoreBinary(identity(), cache, destination, bundleManifest)
      ).toThrow('unexpected files');
    });
  });

  it('verifies the per-run bundle against source and platform identity', () => {
    withTempDirectory((directory) => {
      const source = path.join(directory, 'chat_cli');
      const cache = path.join(directory, 'cache');
      const bundleManifest = path.join(directory, 'manifest.json');
      fs.writeFileSync(source, 'known binary bytes');
      stageBinary(identity(), source, cache, bundleManifest);

      expect(
        prepareBundle(source, bundleManifest, identity().source, 'Linux', 'X64')
          .binary.name
      ).toBe('chat_cli');
      if (process.platform !== 'win32') {
        expect(fs.statSync(source).mode & 0o111).not.toBe(0);
      }

      const changedSource = structuredClone(identity().source);
      changedSource.indexSha256 = 'b'.repeat(64);
      expect(() =>
        verifyBundle(source, bundleManifest, changedSource, 'Linux', 'X64')
      ).toThrow('different Rust inputs');
      expect(() =>
        verifyBundle(
          source,
          bundleManifest,
          identity().source,
          'Windows',
          'X64'
        )
      ).toThrow('different runner platform');
    });
  });

  it('rejects a bundle built with a different Cargo recipe', () => {
    withTempDirectory((directory) => {
      const source = path.join(directory, 'chat_cli');
      const cache = path.join(directory, 'cache');
      const bundleManifest = path.join(directory, 'manifest.json');
      const changedInput = identityInput();
      changedInput.build.args.push('--features', 'unexpected');
      fs.writeFileSync(source, 'known binary bytes');
      stageBinary(
        createBinaryIdentity(changedInput),
        source,
        cache,
        bundleManifest
      );

      expect(() =>
        verifyBundle(
          source,
          bundleManifest,
          changedInput.source,
          'Linux',
          'X64'
        )
      ).toThrow('different Cargo build recipe');
    });
  });
});
