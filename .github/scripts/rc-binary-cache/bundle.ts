import * as crypto from 'node:crypto';
import * as fs from 'node:fs';
import * as path from 'node:path';

import {
  MANIFEST_NAME,
  MANIFEST_SCHEMA,
  assertRegularFile,
  identityDigest,
  sha256File,
  stableStringify,
  type BinaryIdentity,
  type BinaryManifest,
  type SourceIdentity,
} from './core';
import { buildPlan } from './identity';

export function writeFileAtomic(filePath: string, contents: string): void {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  const temporaryPath = `${filePath}.tmp-${process.pid}-${crypto.randomUUID()}`;
  fs.writeFileSync(temporaryPath, contents, { encoding: 'utf8', mode: 0o600 });
  if (fs.existsSync(filePath)) {
    fs.rmSync(filePath);
  }
  fs.renameSync(temporaryPath, filePath);
}

function copyFileAtomic(
  sourcePath: string,
  destinationPath: string,
  expectedSha256: string
): void {
  fs.mkdirSync(path.dirname(destinationPath), { recursive: true });
  const temporaryPath = `${destinationPath}.tmp-${process.pid}-${crypto.randomUUID()}`;
  try {
    fs.copyFileSync(sourcePath, temporaryPath, fs.constants.COPYFILE_EXCL);
    if (sha256File(temporaryPath) !== expectedSha256) {
      throw new Error('Binary changed while it was being copied');
    }
    if (process.platform !== 'win32') {
      fs.chmodSync(temporaryPath, 0o755);
    }
    if (fs.existsSync(destinationPath)) {
      fs.rmSync(destinationPath);
    }
    fs.renameSync(temporaryPath, destinationPath);
  } finally {
    if (fs.existsSync(temporaryPath)) {
      fs.rmSync(temporaryPath);
    }
  }
}

function parseManifest(manifestPath: string): BinaryManifest {
  const stat = assertRegularFile(manifestPath, 'Cache manifest');
  if (stat.size > 1024 * 1024) {
    throw new Error('Cache manifest exceeds the 1 MiB limit');
  }

  const value: unknown = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  if (value === null || typeof value !== 'object') {
    throw new Error('Cache manifest must be an object');
  }
  const manifest = value as Partial<BinaryManifest>;
  if (
    manifest.schema !== MANIFEST_SCHEMA ||
    typeof manifest.identitySha256 !== 'string' ||
    manifest.identity === undefined ||
    manifest.binary === undefined ||
    typeof manifest.binary.name !== 'string' ||
    typeof manifest.binary.sha256 !== 'string' ||
    typeof manifest.binary.size !== 'number'
  ) {
    throw new Error('Cache manifest has an unsupported shape');
  }
  if (!/^[a-f0-9]{64}$/.test(manifest.identitySha256)) {
    throw new Error('Cache manifest identity digest is invalid');
  }
  if (!/^[a-f0-9]{64}$/.test(manifest.binary.sha256)) {
    throw new Error('Cache manifest binary digest is invalid');
  }
  if (
    !Number.isSafeInteger(manifest.binary.size) ||
    manifest.binary.size <= 0
  ) {
    throw new Error('Cache manifest binary size is invalid');
  }
  return manifest as BinaryManifest;
}

function validateManifestIdentity(
  manifest: BinaryManifest,
  identity: BinaryIdentity
): void {
  const expectedIdentitySha256 = identityDigest(identity);
  if (manifest.identitySha256 !== expectedIdentitySha256) {
    throw new Error('Cache manifest identity digest does not match this build');
  }
  if (identityDigest(manifest.identity) !== manifest.identitySha256) {
    throw new Error('Cache manifest identity is internally inconsistent');
  }
  if (stableStringify(manifest.identity) !== stableStringify(identity)) {
    throw new Error('Cache manifest identity does not match this build');
  }
}

function validateBinary(
  binaryPath: string,
  manifest: BinaryManifest,
  expectedName: string
): void {
  if (
    manifest.binary.name !== expectedName ||
    path.basename(binaryPath) !== expectedName ||
    expectedName.includes('/') ||
    expectedName.includes('\\')
  ) {
    throw new Error('Cache manifest binary name is invalid');
  }
  const stat = assertRegularFile(binaryPath, 'Cached binary');
  if (stat.size !== manifest.binary.size) {
    throw new Error('Cached binary size does not match its manifest');
  }
  if (sha256File(binaryPath) !== manifest.binary.sha256) {
    throw new Error('Cached binary SHA-256 does not match its manifest');
  }
}

export function stageBinary(
  identity: BinaryIdentity,
  binaryPath: string,
  cacheDirectory: string,
  bundleManifestPath: string
): BinaryManifest {
  const expectedName = identity.build.binaryName;
  const sourceStat = assertRegularFile(binaryPath, 'Built binary');
  if (path.basename(binaryPath) !== expectedName || sourceStat.size <= 0) {
    throw new Error(`Built binary name does not match ${expectedName}`);
  }

  fs.rmSync(cacheDirectory, { force: true, recursive: true });
  fs.mkdirSync(cacheDirectory, { recursive: true });
  const cachedBinaryPath = path.join(cacheDirectory, expectedName);
  fs.copyFileSync(binaryPath, cachedBinaryPath, fs.constants.COPYFILE_EXCL);
  if (process.platform !== 'win32') {
    fs.chmodSync(cachedBinaryPath, 0o755);
  }

  const cachedStat = assertRegularFile(cachedBinaryPath, 'Staged binary');
  const manifest: BinaryManifest = {
    schema: MANIFEST_SCHEMA,
    identitySha256: identityDigest(identity),
    identity,
    binary: {
      name: expectedName,
      size: cachedStat.size,
      sha256: sha256File(cachedBinaryPath),
    },
  };
  const manifestContents = `${stableStringify(manifest)}\n`;
  writeFileAtomic(path.join(cacheDirectory, MANIFEST_NAME), manifestContents);
  writeFileAtomic(bundleManifestPath, manifestContents);
  return manifest;
}

export function restoreBinary(
  identity: BinaryIdentity,
  cacheDirectory: string,
  destinationPath: string,
  bundleManifestPath: string
): BinaryManifest {
  const directoryStat = fs.lstatSync(cacheDirectory);
  if (!directoryStat.isDirectory() || directoryStat.isSymbolicLink()) {
    throw new Error('Binary cache path must be a regular directory');
  }

  const expectedEntries = [MANIFEST_NAME, identity.build.binaryName].sort();
  const actualEntries = fs.readdirSync(cacheDirectory).sort();
  if (stableStringify(actualEntries) !== stableStringify(expectedEntries)) {
    throw new Error('Binary cache contains unexpected files');
  }

  const manifestPath = path.join(cacheDirectory, MANIFEST_NAME);
  const binaryPath = path.join(cacheDirectory, identity.build.binaryName);
  const manifest = parseManifest(manifestPath);
  validateManifestIdentity(manifest, identity);
  validateBinary(binaryPath, manifest, identity.build.binaryName);

  copyFileAtomic(binaryPath, destinationPath, manifest.binary.sha256);
  writeFileAtomic(bundleManifestPath, `${stableStringify(manifest)}\n`);
  return manifest;
}

export function verifyBundle(
  binaryPath: string,
  manifestPath: string,
  source: SourceIdentity,
  runnerOS: string,
  runnerArch: string
): BinaryManifest {
  const manifest = parseManifest(manifestPath);
  if (identityDigest(manifest.identity) !== manifest.identitySha256) {
    throw new Error('Bundle manifest identity is internally inconsistent');
  }
  if (stableStringify(manifest.identity.source) !== stableStringify(source)) {
    throw new Error('Bundle binary was built from different Rust inputs');
  }
  if (
    manifest.identity.runner.os !== runnerOS ||
    manifest.identity.runner.arch !== runnerArch
  ) {
    throw new Error('Bundle binary targets a different runner platform');
  }
  if (
    stableStringify(manifest.identity.build) !==
    stableStringify(buildPlan(runnerOS))
  ) {
    throw new Error('Bundle binary used a different Cargo build recipe');
  }
  validateBinary(binaryPath, manifest, path.basename(binaryPath));
  return manifest;
}

export function prepareBundle(
  binaryPath: string,
  manifestPath: string,
  source: SourceIdentity,
  runnerOS: string,
  runnerArch: string
): BinaryManifest {
  assertRegularFile(binaryPath, 'Downloaded binary');
  if (process.platform !== 'win32') {
    fs.chmodSync(binaryPath, 0o755);
  }
  return verifyBundle(binaryPath, manifestPath, source, runnerOS, runnerArch);
}
