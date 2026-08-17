import * as crypto from 'node:crypto';
import * as fs from 'node:fs';

import {
  CACHE_KEY_SCHEMA,
  classifyRestore,
  firstDifferencePath,
  identityDigest,
  stableStringify,
  type BinaryIdentity,
} from './core';
import {
  assertIdentityUnchanged,
  buildPlan,
  cacheEnabled,
  collectCurrentIdentity,
  collectSourceIdentity,
  environmentValue,
  executeBuildPlan,
} from './identity';
import {
  prepareBundle,
  restoreBinary,
  stageBinary,
  writeFileAtomic,
} from './bundle';

function parseArguments(argv: string[]): {
  command: string;
  values: Record<string, string>;
} {
  const [command, ...rest] = argv;
  if (command === undefined) {
    throw new Error('A command is required');
  }
  const values: Record<string, string> = {};
  for (let index = 0; index < rest.length; index += 2) {
    const name = rest[index];
    const value = rest[index + 1];
    if (name === undefined || !name.startsWith('--') || value === undefined) {
      throw new Error(`Invalid argument near ${name ?? '<end>'}`);
    }
    values[name.slice(2)] = value;
  }
  return { command, values };
}

function required(values: Record<string, string>, name: string): string {
  const value = values[name];
  if (value === undefined || value.length === 0) {
    throw new Error(`--${name} is required`);
  }
  return value;
}

function readIdentity(identityPath: string): BinaryIdentity {
  return JSON.parse(fs.readFileSync(identityPath, 'utf8')) as BinaryIdentity;
}

function writeOutput(name: string, value: string): void {
  const outputPath = process.env.GITHUB_OUTPUT;
  if (outputPath === undefined) {
    console.log(`${name}=${value}`);
    return;
  }
  const delimiter = `RC_BINARY_CACHE_${crypto.randomUUID()}`;
  fs.appendFileSync(
    outputPath,
    `${name}<<${delimiter}\n${value}\n${delimiter}\n`,
    'utf8'
  );
}

function appendSummary(lines: string[]): void {
  const summaryPath = process.env.GITHUB_STEP_SUMMARY;
  if (summaryPath !== undefined) {
    fs.appendFileSync(summaryPath, `${lines.join('\n')}\n`, 'utf8');
  }
}

export async function main(): Promise<void> {
  const { command, values } = parseArguments(process.argv.slice(2));

  if (command === 'build') {
    const identity = readIdentity(required(values, 'identity-path'));
    const runnerOS = environmentValue('RUNNER_OS');
    if (runnerOS === undefined) {
      throw new Error('RUNNER_OS is required');
    }
    const difference = firstDifferencePath(
      identity.build,
      buildPlan(runnerOS),
      'build'
    );
    if (difference !== undefined) {
      throw new Error(`Recorded build plan does not match at ${difference}`);
    }
    executeBuildPlan(identity.build);
    return;
  }

  if (command === 'prepare') {
    const runnerLabel = required(values, 'runner-label');
    const identityPath = required(values, 'identity-path');
    const cacheDirectory = required(values, 'cache-directory');
    const identity = collectCurrentIdentity(runnerLabel);
    const digest = identityDigest(identity);
    const enabled = cacheEnabled(identity);

    fs.rmSync(cacheDirectory, { force: true, recursive: true });
    writeFileAtomic(identityPath, `${stableStringify(identity)}\n`);
    writeOutput('cache-enabled', String(enabled));
    writeOutput('cache-key', `rc-chat-cli-${CACHE_KEY_SCHEMA}-${digest}`);
    writeOutput('cache-directory', cacheDirectory);
    writeOutput('identity-path', identityPath);
    writeOutput('identity-sha256', digest);
    writeOutput('binary-name', identity.build.binaryName);
    writeOutput('binary-path', identity.build.binaryPath);
    appendSummary([
      '### RC binary identity',
      '',
      `- Digest: \`${digest}\``,
      `- Source files: \`${identity.source.fileCount}\``,
      `- Runner image: \`${identity.runner.imageOS} ${identity.runner.imageVersion}\``,
      `- Native toolchain: \`${identity.toolchain.nativeComplete ? 'complete' : 'incomplete'}\``,
      `- Cross-run cache: \`${enabled ? 'enabled' : 'disabled (build identity incomplete)'}\``,
      '',
    ]);
    return;
  }

  if (command === 'restore') {
    const enabled = required(values, 'enabled') === 'true';
    const outcome = required(values, 'outcome');
    const cacheHit = values['cache-hit'] ?? '';
    const cacheDirectory = required(values, 'cache-directory');
    const identityPath = required(values, 'identity-path');
    const destinationPath = required(values, 'destination-path');
    const bundleManifestPath = required(values, 'bundle-manifest-path');
    const decision = classifyRestore(enabled, outcome, cacheHit);

    if (decision.kind === 'restore') {
      const identity = readIdentity(identityPath);
      assertIdentityUnchanged(identity, identity.runner.label);
      const manifest = restoreBinary(
        identity,
        cacheDirectory,
        destinationPath,
        bundleManifestPath
      );
      writeOutput('rebuild', 'false');
      writeOutput('decision', 'exact-hit');
      appendSummary([
        '### RC binary cache',
        '',
        `Validated exact hit: \`${manifest.binary.sha256}\``,
        '',
      ]);
      return;
    }

    fs.rmSync(cacheDirectory, { force: true, recursive: true });
    writeOutput('rebuild', 'true');
    writeOutput('decision', decision.reason);
    appendSummary([
      '### RC binary cache',
      '',
      `Rebuild required: \`${decision.reason}\``,
      '',
    ]);
    return;
  }

  if (command === 'stage') {
    const identityPath = required(values, 'identity-path');
    const binaryPath = required(values, 'binary-path');
    const cacheDirectory = required(values, 'cache-directory');
    const bundleManifestPath = required(values, 'bundle-manifest-path');
    const identity = readIdentity(identityPath);
    assertIdentityUnchanged(identity, identity.runner.label);
    const manifest = stageBinary(
      identity,
      binaryPath,
      cacheDirectory,
      bundleManifestPath
    );
    writeOutput('binary-sha256', manifest.binary.sha256);
    appendSummary([
      '### RC binary build',
      '',
      `Staged binary: \`${manifest.binary.sha256}\``,
      '',
    ]);
    return;
  }

  if (command === 'verify-bundle') {
    const binaryPath = required(values, 'binary-path');
    const manifestPath = required(values, 'manifest-path');
    const runnerOS = environmentValue('RUNNER_OS');
    const runnerArch = environmentValue('RUNNER_ARCH');
    if (runnerOS === undefined || runnerArch === undefined) {
      throw new Error('RUNNER_OS and RUNNER_ARCH are required');
    }
    const manifest = prepareBundle(
      binaryPath,
      manifestPath,
      collectSourceIdentity(true),
      runnerOS,
      runnerArch
    );
    console.log(
      `Verified ${binaryPath} (${manifest.binary.sha256}, ${manifest.identitySha256})`
    );
    return;
  }

  throw new Error(`Unsupported command: ${command}`);
}
