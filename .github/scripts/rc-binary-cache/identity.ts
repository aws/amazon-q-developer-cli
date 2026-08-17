import * as fs from 'node:fs';

import {
  COMPILE_ENV_NAMES,
  COMPILE_ENV_PATTERNS,
  SOURCE_PATHS,
  createBinaryIdentity,
  findForbiddenEnvironment,
  firstDifferencePath,
  hashSourceIndex,
  hashTrackedFiles,
  type BinaryIdentity,
  type BuildPlan,
  type SourceIdentity,
} from './core';

function outputText(result: Bun.SpawnSyncReturns<Uint8Array>): string {
  return `${Buffer.from(result.stdout).toString('utf8')}${Buffer.from(
    result.stderr
  ).toString('utf8')}`.trim();
}

export type ToolOutputStream = 'stderr' | 'stdout';

export function stableToolOutput(
  stdout: string,
  stderr: string,
  stream: ToolOutputStream = 'stdout'
): string {
  const output = (stream === 'stdout' ? stdout : stderr).trim();
  if (output.length === 0) {
    const diagnostic = (stream === 'stdout' ? stderr : stdout).trim();
    throw new Error(
      `Tool produced no ${stream}${diagnostic.length > 0 ? `: ${diagnostic}` : ''}`
    );
  }
  return output;
}

function run(command: string[], label = command.join(' ')): string {
  const result = Bun.spawnSync({
    cmd: command,
    stdout: 'pipe',
    stderr: 'pipe',
    env: process.env,
  });
  if (result.exitCode !== 0) {
    throw new Error(
      `${label} failed (${result.exitCode}): ${outputText(result)}`
    );
  }
  return outputText(result);
}

function runStableTool(
  command: string[],
  stream: ToolOutputStream = 'stdout'
): string {
  const result = Bun.spawnSync({
    cmd: command,
    stdout: 'pipe',
    stderr: 'pipe',
    env: process.env,
  });
  if (result.exitCode !== 0) {
    throw new Error(
      `${command.join(' ')} failed (${result.exitCode}): ${outputText(result)}`
    );
  }
  return stableToolOutput(
    Buffer.from(result.stdout).toString('utf8'),
    Buffer.from(result.stderr).toString('utf8'),
    stream
  );
}

function runOptionalStableTool(
  command: string[],
  stream: ToolOutputStream = 'stdout'
): string {
  try {
    return runStableTool(command, stream);
  } catch (error) {
    return `unavailable: ${error instanceof Error ? error.message : String(error)}`;
  }
}

export function collectSourceIdentity(requireClean: boolean): SourceIdentity {
  const assertClean = (): void => {
    const status = run(
      [
        'git',
        'status',
        '--porcelain=v1',
        '-z',
        '--untracked-files=all',
        '--',
        ...SOURCE_PATHS,
      ],
      'git status for Rust build inputs'
    );
    if (status.length > 0) {
      throw new Error('Rust build inputs must match the checked-out Git index');
    }
  };
  if (requireClean) {
    assertClean();
  }

  const result = Bun.spawnSync({
    cmd: ['git', 'ls-files', '-s', '-z', '--', ...SOURCE_PATHS],
    stdout: 'pipe',
    stderr: 'pipe',
    env: process.env,
  });
  if (result.exitCode !== 0) {
    throw new Error(
      `git ls-files failed (${result.exitCode}): ${outputText(result)}`
    );
  }
  const index = Buffer.from(result.stdout);
  const fileCount = index.filter((byte) => byte === 0).length;
  if (fileCount === 0) {
    throw new Error('No tracked Rust build inputs were found');
  }
  const contentSha256 = hashTrackedFiles(index);
  if (requireClean) {
    assertClean();
  }
  return {
    contentSha256,
    fileCount,
    gitObjectFormat: runStableTool([
      'git',
      'rev-parse',
      '--show-object-format',
    ]),
    indexSha256: hashSourceIndex(index),
    paths: SOURCE_PATHS,
  };
}

export function environmentValue(name: string): string | undefined {
  const direct = process.env[name];
  if (direct !== undefined || process.platform !== 'win32') {
    return direct;
  }
  const match = Object.keys(process.env).find(
    (candidate) => candidate.toLowerCase() === name.toLowerCase()
  );
  return match === undefined ? undefined : process.env[match];
}

function collectCompileEnvironment(): Record<string, string | null> {
  const names = new Set<string>(COMPILE_ENV_NAMES);
  for (const name of Object.keys(process.env)) {
    if (COMPILE_ENV_PATTERNS.some((pattern) => pattern.test(name))) {
      names.add(name);
    }
  }
  return Object.fromEntries(
    [...names]
      .sort((left, right) => left.localeCompare(right))
      .map((name) => [name, environmentValue(name) ?? null])
  );
}

function collectSystemIdentity(runnerOS: string): string {
  switch (runnerOS) {
    case 'Linux':
      return fs.readFileSync('/etc/os-release', 'utf8').trim();
    case 'macOS':
      return runStableTool(['sw_vers']);
    case 'Windows':
      return runStableTool([
        'powershell.exe',
        '-NoProfile',
        '-NonInteractive',
        '-Command',
        [
          '$os = Get-CimInstance Win32_OperatingSystem',
          'Write-Output "$($os.Caption)|$($os.Version)|$($os.BuildNumber)"',
        ].join('; '),
      ]);
    default:
      throw new Error(`Unsupported runner OS: ${runnerOS}`);
  }
}

function collectNativeToolchain(runnerOS: string): Record<string, string> {
  switch (runnerOS) {
    case 'Linux':
      return {
        cc: runOptionalStableTool(['cc', '--version']),
        ld: runOptionalStableTool(['ld', '--version']),
        alsa: runOptionalStableTool(['pkg-config', '--modversion', 'alsa']),
        alsaPackage: runOptionalStableTool([
          'dpkg-query',
          '--show',
          '--showformat=${binary:Package}|${Version}|${Architecture}',
          'libasound2-dev',
        ]),
      };
    case 'macOS':
      return {
        clang: runOptionalStableTool(['clang', '--version']),
        ld: runOptionalStableTool(['ld', '-v'], 'stderr'),
        protobuf: runOptionalStableTool(['protoc', '--version']),
        protobufPackage: runOptionalStableTool([
          'brew',
          'list',
          '--versions',
          'protobuf',
        ]),
        xcode: runOptionalStableTool(['xcodebuild', '-version']),
      };
    case 'Windows':
      return {
        msvc: runOptionalStableTool([
          'powershell.exe',
          '-NoProfile',
          '-NonInteractive',
          '-Command',
          [
            '$link = (Get-Command link.exe -ErrorAction Stop).Source',
            '$version = (Get-Item $link).VersionInfo.FileVersion',
            'Write-Output "$link|$version"',
          ].join('; '),
        ]),
      };
    default:
      throw new Error(`Unsupported runner OS: ${runnerOS}`);
  }
}

function nativeToolchainComplete(native: Record<string, string>): boolean {
  return Object.values(native).every(
    (value) => !value.startsWith('unavailable:')
  );
}

export function buildPlan(runnerOS: string): BuildPlan {
  const windows = runnerOS === 'Windows';
  return {
    program: 'cargo',
    args: [
      'build',
      '-p',
      'chat_cli',
      '--release',
      ...(windows ? ['--no-default-features'] : []),
      '--locked',
    ],
    package: 'chat_cli',
    profile: 'release',
    binaryName: windows ? 'chat_cli.exe' : 'chat_cli',
    binaryPath: windows
      ? 'target/release/chat_cli.exe'
      : 'target/release/chat_cli',
  };
}

export function buildCommand(plan: BuildPlan): string[] {
  return [plan.program, ...plan.args];
}

export function executeBuildPlan(plan: BuildPlan): void {
  const command = buildCommand(plan);
  const result = Bun.spawnSync({
    cmd: command,
    stdin: 'inherit',
    stdout: 'inherit',
    stderr: 'inherit',
    env: process.env,
  });
  if (result.exitCode !== 0) {
    throw new Error(
      `${command.join(' ')} failed with exit code ${result.exitCode}`
    );
  }
}

export function collectCurrentIdentity(
  runnerLabel: string,
  requireClean = true
): BinaryIdentity {
  const runnerOS = environmentValue('RUNNER_OS');
  const runnerArch = environmentValue('RUNNER_ARCH');
  if (runnerOS === undefined || runnerArch === undefined) {
    throw new Error('RUNNER_OS and RUNNER_ARCH are required');
  }

  const forbidden = findForbiddenEnvironment(process.env);
  if (forbidden.length > 0) {
    throw new Error(
      `RC binaries must be asset-free; unset: ${forbidden.join(', ')}`
    );
  }

  const native = collectNativeToolchain(runnerOS);
  return createBinaryIdentity({
    source: collectSourceIdentity(requireClean),
    runner: {
      label: runnerLabel,
      os: runnerOS,
      arch: runnerArch,
      imageOS: environmentValue('ImageOS') ?? 'unavailable',
      imageVersion: environmentValue('ImageVersion') ?? 'unavailable',
      system: collectSystemIdentity(runnerOS),
    },
    toolchain: {
      rustc: runStableTool(['rustc', '-vV']),
      cargo: runStableTool(['cargo', '-Vv']),
      native,
      nativeComplete: nativeToolchainComplete(native),
    },
    build: buildPlan(runnerOS),
    environment: collectCompileEnvironment(),
  });
}

export function assertIdentityUnchanged(
  expected: BinaryIdentity,
  runnerLabel: string
): void {
  const current = collectCurrentIdentity(runnerLabel);
  const difference = firstDifferencePath(expected, current);
  if (difference !== undefined) {
    throw new Error(`Rust build identity changed at ${difference}`);
  }
}

export function cacheEnabled(identity: BinaryIdentity): boolean {
  return (
    identity.runner.imageOS !== 'unavailable' &&
    identity.runner.imageVersion !== 'unavailable' &&
    identity.toolchain.nativeComplete
  );
}
