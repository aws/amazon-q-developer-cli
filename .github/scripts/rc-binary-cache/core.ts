import * as crypto from 'node:crypto';
import * as fs from 'node:fs';
import * as path from 'node:path';

export const SOURCE_PATHS = [
  'Cargo.toml',
  'Cargo.lock',
  'rust-toolchain.toml',
  '.cargo/**',
  'crates/**',
  'autodocs/**',
  'autodocs-v2/**',
  '.github/workflows/rc-certification.yml',
  '.github/scripts/rc-binary-cache.ts',
  '.github/scripts/rc-binary-cache/**',
] as const;

export const ASSET_ENV_CONTRACT_PATH = 'crates/asset-embedding-env.txt';

function loadAssetEmbeddingEnvironment(): readonly string[] {
  const contents = fs.readFileSync(
    path.resolve(import.meta.dir, '../../..', ASSET_ENV_CONTRACT_PATH),
    'utf8'
  );
  const names = contents
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith('#'));
  if (
    names.length === 0 ||
    names.some((name) => !/^[A-Z][A-Z0-9_]+$/.test(name)) ||
    new Set(names).size !== names.length
  ) {
    throw new Error(
      `Invalid asset environment contract: ${ASSET_ENV_CONTRACT_PATH}`
    );
  }
  return Object.freeze(names);
}

export const FORBIDDEN_EMBEDDING_ENV = loadAssetEmbeddingEnvironment();

export const COMPILE_ENV_NAMES = [
  'AMAZON_Q_BUILD_DATETIME',
  'AMAZON_Q_BUILD_HASH',
  'AR',
  'BINDGEN_EXTRA_CLANG_ARGS',
  'CARGO_BUILD_JOBS',
  'CARGO_BUILD_RUSTC',
  'CARGO_BUILD_RUSTC_WRAPPER',
  'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER',
  'CARGO_BUILD_TARGET',
  'CARGO_ENCODED_RUSTFLAGS',
  'CARGO_HOME',
  'CARGO_INCREMENTAL',
  'CARGO_TARGET_DIR',
  'CC',
  'CFLAGS',
  'CXX',
  'CXXFLAGS',
  'DEVELOPER_DIR',
  'HOST_AR',
  'HOST_CC',
  'HOST_CFLAGS',
  'HOST_CXX',
  'HOST_CXXFLAGS',
  'KIRO_VERSION',
  'LDFLAGS',
  'LIBCLANG_PATH',
  'MACOSX_DEPLOYMENT_TARGET',
  'OPENSSL_DIR',
  'OPENSSL_LIB_DIR',
  'OPENSSL_NO_VENDOR',
  'OPENSSL_STATIC',
  'PKG_CONFIG_ALLOW_CROSS',
  'PKG_CONFIG_PATH',
  'PROTOC',
  'RC_SCCACHE_VERSION',
  'RUSTC',
  'RUSTC_WRAPPER',
  'RUSTC_WORKSPACE_WRAPPER',
  'RUSTFLAGS',
  'RUSTUP_HOME',
  'RUSTUP_TOOLCHAIN',
  'SCCACHE_GHA_ENABLED',
  'SCCACHE_PATH',
  'SDKROOT',
  'SOURCE_DATE_EPOCH',
  'VCPKG_ROOT',
] as const;

export const COMPILE_ENV_PATTERNS = [
  /^AR_.+$/,
  /^CARGO_PROFILE_.+$/,
  /^CARGO_TARGET_.+_(LINKER|RUNNER|RUSTFLAGS)$/,
  /^CC_.+$/,
  /^CFLAGS_.+$/,
  /^CXX_.+$/,
  /^CXXFLAGS_.+$/,
  /^LDFLAGS_.+$/,
  /^PKG_CONFIG_.+$/,
] as const;

export const MANIFEST_NAME = 'manifest.json';
export const MANIFEST_SCHEMA = 1;
export const CACHE_KEY_SCHEMA = 'v1';

export interface SourceIdentity {
  contentSha256: string;
  fileCount: number;
  gitObjectFormat: string;
  indexSha256: string;
  paths: readonly string[];
}

export interface RunnerIdentity {
  arch: string;
  imageOS: string;
  imageVersion: string;
  label: string;
  os: string;
  system: string;
}

export interface ToolchainIdentity {
  cargo: string;
  native: Record<string, string>;
  nativeComplete: boolean;
  rustc: string;
}

export interface BuildPlan {
  args: string[];
  binaryName: string;
  binaryPath: string;
  package: 'chat_cli';
  program: 'cargo';
  profile: 'release';
}

export interface BinaryIdentity {
  build: BuildPlan;
  environment: Record<string, string | null>;
  runner: RunnerIdentity;
  schema: 1;
  source: SourceIdentity;
  toolchain: ToolchainIdentity;
}

export interface BinaryManifest {
  binary: {
    name: string;
    sha256: string;
    size: number;
  };
  identity: BinaryIdentity;
  identitySha256: string;
  schema: 1;
}

export interface BinaryIdentityInput {
  build: BuildPlan;
  environment: Record<string, string | null>;
  runner: RunnerIdentity;
  source: SourceIdentity;
  toolchain: ToolchainIdentity;
}

export type RestoreDecision =
  | {
      kind: 'rebuild';
      reason: 'cache-disabled' | 'cache-miss' | 'restore-error';
    }
  | { kind: 'restore'; reason: 'exact-hit' };

function sortValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(sortValue);
  }
  if (value !== null && typeof value === 'object') {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, child]) => [key, sortValue(child)])
    );
  }
  return value;
}

export function stableStringify(value: unknown): string {
  return JSON.stringify(sortValue(value));
}

export function firstDifferencePath(
  left: unknown,
  right: unknown,
  location = 'identity'
): string | undefined {
  if (Object.is(left, right)) {
    return undefined;
  }
  if (Array.isArray(left) || Array.isArray(right)) {
    if (!Array.isArray(left) || !Array.isArray(right)) {
      return location;
    }
    if (left.length !== right.length) {
      return `${location}.length`;
    }
    for (let index = 0; index < left.length; index += 1) {
      const difference = firstDifferencePath(
        left[index],
        right[index],
        `${location}[${index}]`
      );
      if (difference !== undefined) {
        return difference;
      }
    }
    return undefined;
  }
  if (
    left !== null &&
    right !== null &&
    typeof left === 'object' &&
    typeof right === 'object'
  ) {
    const leftRecord = left as Record<string, unknown>;
    const rightRecord = right as Record<string, unknown>;
    const keys = [
      ...new Set([...Object.keys(leftRecord), ...Object.keys(rightRecord)]),
    ].sort((leftKey, rightKey) => leftKey.localeCompare(rightKey));
    for (const key of keys) {
      if (!(key in leftRecord) || !(key in rightRecord)) {
        return `${location}.${key}`;
      }
      const difference = firstDifferencePath(
        leftRecord[key],
        rightRecord[key],
        `${location}.${key}`
      );
      if (difference !== undefined) {
        return difference;
      }
    }
    return undefined;
  }
  return location;
}

export function sha256Bytes(value: Uint8Array | string): string {
  return crypto.createHash('sha256').update(value).digest('hex');
}

export function sha256File(filePath: string): string {
  const hash = crypto.createHash('sha256');
  const descriptor = fs.openSync(filePath, 'r');
  const buffer = Buffer.allocUnsafe(1024 * 1024);
  try {
    let bytesRead = 0;
    do {
      bytesRead = fs.readSync(descriptor, buffer, 0, buffer.length, null);
      if (bytesRead > 0) {
        hash.update(buffer.subarray(0, bytesRead));
      }
    } while (bytesRead > 0);
  } finally {
    fs.closeSync(descriptor);
  }
  return hash.digest('hex');
}

export function createBinaryIdentity(
  input: BinaryIdentityInput
): BinaryIdentity {
  return {
    schema: 1,
    source: input.source,
    runner: input.runner,
    toolchain: input.toolchain,
    build: input.build,
    environment: Object.fromEntries(
      Object.entries(input.environment).sort(([left], [right]) =>
        left.localeCompare(right)
      )
    ),
  };
}

export function identityDigest(identity: BinaryIdentity): string {
  return sha256Bytes(stableStringify(identity));
}

export function hashSourceIndex(index: Uint8Array): string {
  return sha256Bytes(index);
}

export function hashTrackedFiles(
  index: Uint8Array,
  workspace = process.cwd()
): string {
  const hash = crypto.createHash('sha256');
  const entries = Buffer.from(index)
    .toString('utf8')
    .split('\0')
    .filter((entry) => entry.length > 0);
  const buffer = Buffer.allocUnsafe(1024 * 1024);

  for (const entry of entries) {
    const separator = entry.indexOf('\t');
    if (separator < 0) {
      throw new Error('Tracked Git entry is malformed');
    }
    const relativePath = entry.slice(separator + 1);
    const filePath = path.resolve(workspace, relativePath);
    const resolvedRelativePath = path.relative(workspace, filePath);
    if (
      resolvedRelativePath === '..' ||
      resolvedRelativePath.startsWith(`..${path.sep}`) ||
      path.isAbsolute(resolvedRelativePath)
    ) {
      throw new Error(
        `Tracked Git path escapes the workspace: ${relativePath}`
      );
    }

    const stat = assertRegularFile(filePath, 'Tracked Rust build input');
    hash.update(`entry:${Buffer.byteLength(entry)}\0${entry}`);
    hash.update(`size:${stat.size}\0`);

    const descriptor = fs.openSync(filePath, 'r');
    try {
      let bytesRead = 0;
      do {
        bytesRead = fs.readSync(descriptor, buffer, 0, buffer.length, null);
        if (bytesRead > 0) {
          hash.update(buffer.subarray(0, bytesRead));
        }
      } while (bytesRead > 0);
    } finally {
      fs.closeSync(descriptor);
    }
  }
  return hash.digest('hex');
}

export function findForbiddenEnvironment(
  environment: Record<string, string | undefined>
): string[] {
  const names = new Set(
    Object.keys(environment).map((name) => name.toLowerCase())
  );
  return FORBIDDEN_EMBEDDING_ENV.filter((name) =>
    names.has(name.toLowerCase())
  );
}

export function classifyRestore(
  enabled: boolean,
  outcome: string,
  cacheHit: string
): RestoreDecision {
  if (!enabled) {
    return { kind: 'rebuild', reason: 'cache-disabled' };
  }
  if (outcome !== 'success') {
    return { kind: 'rebuild', reason: 'restore-error' };
  }
  if (cacheHit === 'true') {
    return { kind: 'restore', reason: 'exact-hit' };
  }
  if (cacheHit === '' || cacheHit === 'false') {
    return { kind: 'rebuild', reason: 'cache-miss' };
  }
  throw new Error(`Unexpected actions/cache cache-hit value: ${cacheHit}`);
}

export function assertRegularFile(filePath: string, label: string): fs.Stats {
  const stat = fs.lstatSync(filePath);
  if (!stat.isFile() || stat.isSymbolicLink()) {
    throw new Error(`${label} must be a regular file: ${filePath}`);
  }
  return stat;
}
