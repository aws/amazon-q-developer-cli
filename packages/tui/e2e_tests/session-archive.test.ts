/**
 * E2E tests for `kiro-cli chat _ export-session` and `chat _ import-session`.
 *
 * These tests pin the JSON output contract that the TUI's eventual
 * `/chat save` and `/chat load` slash command handlers will consume,
 * and validate that on-disk session artifacts produced by the import
 * subcommand conform to the `@kiro/acp-type-covenant` schemas (the same
 * shape KAS itself reads on `session/load`).
 *
 * Why these are spawnSync-based and not E2ETestCase-based: the
 * subcommands are headless, JSON-on-stdout, and never touch the TUI.
 * Driving them through a PTY would only add flake.
 *
 * Output contract:
 * - Success: exit 0, single JSON line `{"success":true,"path":"<abs>"}`
 * - Failure: exit 1, single JSON line `{"success":false,"error":"<msg>"}`
 *
 * Both shapes go to stdout (never stderr) so the caller always parses
 * with `JSON.parse(stdout)` without disambiguating streams.
 */
import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { requireChatCliBin } from '../src/utils/chat-cli-bin';
import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, basename } from 'node:path';
import { PersistedMessageSchema, SessionMetadataSchema, type SessionMetadata } from '@kiro/acp-type-covenant';
import { computeWorkspaceHash } from '@kiro/agent';

const BIN = requireChatCliBin();

// ─── Helpers ─────────────────────────────────────────────────────────

/** Result of running the binary's `chat _ <subcommand>` JSON-on-stdout surface. */
interface RunResult {
  /** Process exit code. `null` only if the child was killed by a signal. */
  exitCode: number | null;
  /** Full captured stdout (logs may precede the JSON line). */
  stdout: string;
  /** Full captured stderr (binary should never write to stderr on the contract path; populated on panics or pre-flight crashes). */
  stderr: string;
  /** Last stdout line parsed as the `{result, path?, error?}` JSON contract. */
  parsed: { success: boolean; path?: string; error?: string };
}

function runCli(args: string[]): RunResult {
  const res = spawnSync(BIN, ['chat', '_', ...args], { encoding: 'utf8' });
  if (res.error) throw res.error;
  const lines = res.stdout.trim().split('\n').filter(Boolean);
  if (lines.length === 0) {
    throw new Error(
      `binary produced no stdout (exit=${res.status}); stderr=${res.stderr}`,
    );
  }
  // The binary may print other lines (logs); the JSON contract is the LAST line.
  const last = lines[lines.length - 1]!;
  let parsed: { success: boolean; path?: string; error?: string };
  try {
    parsed = JSON.parse(last);
  } catch (cause) {
    throw new Error(
      `last stdout line is not JSON: ${JSON.stringify(last)} (exit=${res.status}, stderr=${res.stderr})`,
      { cause },
    );
  }
  return { exitCode: res.status, stdout: res.stdout, stderr: res.stderr, parsed };
}

interface SeedOptions {
  basePath: string;
  workspacePath: string;
  sessionId: string;
  title?: string;
}

const FIXTURE_ROOT = resolve(__dirname, 'test_fixtures', 'canonical-kas-session');

/**
 * Resolve the single `sess_*` subdir under the fixture root. The fixture
 * preserves KAS's on-disk layout: `<root>/sess_<originalId>/{session.json,
 * messages.jsonl, snapshots/}`. There is exactly one such subdir; this
 * helper finds it without hardcoding the captured id.
 */
function findFixtureSessionDir(): string {
  const entries = readdirSync(FIXTURE_ROOT, { withFileTypes: true })
    .filter((e) => e.isDirectory() && e.name.startsWith('sess_'));
  if (entries.length !== 1) {
    throw new Error(
      `expected exactly one sess_* subdir under ${FIXTURE_ROOT}, found ${entries.length}`,
    );
  }
  return join(FIXTURE_ROOT, entries[0]!.name);
}

const FIXTURE_SESSION_DIR = findFixtureSessionDir();

/** Captured KAS `session.json`, parsed once at module load. */
const FIXTURE_SESSION_JSON: Record<string, unknown> = JSON.parse(
  readFileSync(join(FIXTURE_SESSION_DIR, 'session.json'), 'utf8'),
);

/**
 * Raw bytes of the fixture's `messages.jsonl`, copied byte-for-byte into
 * each seeded session dir so the export path sees exactly what KAS wrote
 * during capture.
 */
const FIXTURE_MESSAGES_JSONL: Buffer = readFileSync(join(FIXTURE_SESSION_DIR, 'messages.jsonl'));

/**
 * Deserialized fixture lines. Used only for length and payload-type
 * assertions; the bytes themselves are written verbatim.
 */
const FIXTURE_MESSAGES: { id: string; timestamp: string; payload: { type: string } }[] =
  FIXTURE_MESSAGES_JSONL.toString('utf8')
    .split('\n')
    .filter((l) => l.length > 0)
    .map((l) => JSON.parse(l));

/**
 * Recursively copy `src` to `dst`. Used to clone the fixture's `snapshots/`
 * tree into a freshly seeded session so the export-side filter that excludes
 * `snapshots/` runs against real KAS-shaped data.
 */
function copyRecursive(src: string, dst: string): void {
  if (!existsSync(src)) return;
  mkdirSync(dst, { recursive: true });
  for (const entry of readdirSync(src, { withFileTypes: true })) {
    const s = join(src, entry.name);
    const d = join(dst, entry.name);
    if (entry.isDirectory()) {
      copyRecursive(s, d);
    } else if (entry.isFile()) {
      writeFileSync(d, readFileSync(s));
    }
  }
}

function seedSession(opts: SeedOptions): string {
  const hash = computeWorkspaceHash([opts.workspacePath]);
  const dir = join(opts.basePath, hash, opts.sessionId);
  mkdirSync(dir, { recursive: true });
  const now = new Date().toISOString();
  // Start from the captured KAS session.json verbatim, then overwrite the
  // four fields the test needs to control (id, workspace path, timestamps,
  // optional title). Every other field round-trips as KAS wrote it.
  const sessionJson = {
    ...FIXTURE_SESSION_JSON,
    id: opts.sessionId,
    title: opts.title ?? (FIXTURE_SESSION_JSON.title as string),
    workspacePaths: [opts.workspacePath],
    createdAt: now,
    lastModifiedAt: now,
  };
  writeFileSync(join(dir, 'session.json'), JSON.stringify(sessionJson, null, 2));
  writeFileSync(join(dir, 'messages.jsonl'), FIXTURE_MESSAGES_JSONL);
  copyRecursive(join(FIXTURE_SESSION_DIR, 'snapshots'), join(dir, 'snapshots'));
  return dir;
}

function readJsonl(path: string): unknown[] {
  return readFileSync(path, 'utf8')
    .split('\n')
    .filter((l) => l.length > 0)
    .map((l) => JSON.parse(l));
}

// ─── Tests ───────────────────────────────────────────────────────────

describe('chat _ export-session / import-session', () => {
  let tmp: string;
  let cwd: string; // a stable workspace path (a directory under tmp)

  beforeEach(() => {
    tmp = mkdtempSync(join(tmpdir(), 'kiro-archive-test-'));
    cwd = join(tmp, 'workspace');
    mkdirSync(cwd, { recursive: true });
  });

  afterEach(() => {
    rmSync(tmp, { recursive: true, force: true });
  });

  it('binary exists at expected path (sanity)', () => {
    expect(existsSync(BIN)).toBe(true);
  });

  it('JSON success contract: export emits {result, path} and writes a zip', () => {
    const sessionsRoot = join(tmp, 'sessions');
    seedSession({
      basePath: sessionsRoot,
      workspacePath: cwd,
      sessionId: 'sess_fixture-1',
    });
    const out = join(tmp, 'archive.zip');

    const res = runCli([
      'export-session',
      '--id', 'sess_fixture-1',
      '--cwd', cwd,
      '--out', out,
      '--base-path', sessionsRoot,
    ]);

    expect(res.exitCode).toBe(0);
    expect(res.parsed.success).toBe(true);
    expect(res.parsed.path).toBe(out);
    expect(res.parsed.error).toBeUndefined();
    expect(existsSync(out)).toBe(true);
    expect(statSync(out).size).toBeGreaterThan(0);
  });

  it('JSON error contract: export with unknown id emits {success:false, error}', () => {
    const sessionsRoot = join(tmp, 'sessions');
    mkdirSync(sessionsRoot);
    const out = join(tmp, 'archive.zip');

    const res = runCli([
      'export-session',
      '--id', 'sess_does-not-exist',
      '--cwd', cwd,
      '--out', out,
      '--base-path', sessionsRoot,
    ]);

    expect(res.exitCode).toBe(1);
    expect(res.parsed.success).toBe(false);
    expect(res.parsed.error).toBeTruthy();
    expect(typeof res.parsed.error).toBe('string');
    expect(res.parsed.path).toBeUndefined();
    expect(existsSync(out)).toBe(false);
  });

  it('JSON error contract: import with non-zip file emits {success:false, error}', () => {
    const fakeArchive = join(tmp, 'not-a-zip.zip');
    writeFileSync(fakeArchive, 'totally not a zip file');
    const sessionsRoot = join(tmp, 'sessions');

    const res = runCli([
      'import-session',
      '--archive', fakeArchive,
      '--cwd', cwd,
      '--base-path', sessionsRoot,
    ]);

    expect(res.exitCode).toBe(1);
    expect(res.parsed.success).toBe(false);
    expect(res.parsed.error).toMatch(/zip/i);
  });

  it('roundtrip: export -> import lands a valid session whose contents validate against acp-type-covenant schemas', () => {
    // 1. Seed a session in a source sessions root.
    const sourceRoot = join(tmp, 'src-sessions');
    seedSession({
      basePath: sourceRoot,
      workspacePath: cwd,
      sessionId: 'sess_roundtrip-1',
      title: 'roundtrip fixture',
    });

    // 2. Export to a zip via the binary.
    const archive = join(tmp, 'roundtrip.zip');
    const exportRes = runCli([
      'export-session',
      '--id', 'sess_roundtrip-1',
      '--cwd', cwd,
      '--out', archive,
      '--base-path', sourceRoot,
    ]);
    expect(exportRes.exitCode).toBe(0);
    expect(exportRes.parsed.success).toBe(true);

    // 3. Import into a fresh, empty destination root, under a different cwd
    //    so we exercise the workspacePaths rewrite + bucket re-hash.
    const destRoot = join(tmp, 'dst-sessions');
    const newCwd = join(tmp, 'other-workspace');
    mkdirSync(newCwd, { recursive: true });
    const importRes = runCli([
      'import-session',
      '--archive', archive,
      '--cwd', newCwd,
      '--base-path', destRoot,
    ]);
    expect(importRes.exitCode).toBe(0);
    expect(importRes.parsed.success).toBe(true);
    expect(importRes.parsed.path).toBeTruthy();

    const importedDir = importRes.parsed.path!;
    expect(existsSync(importedDir)).toBe(true);

    // Snapshots are KAS-internal checkpoint state. The export filter must
    // skip them so an imported archive lands clean - the seeded source
    // session has them (copied from the fixture) and the imported one
    // must not.
    expect(existsSync(join(importedDir, 'snapshots'))).toBe(false);

    // The imported dir should sit under the NEW workspace's hash bucket.
    const expectedBucket = computeWorkspaceHash([newCwd]);
    expect(importedDir.startsWith(join(destRoot, expectedBucket))).toBe(true);

    // The imported session id is the basename - a fresh UUID with the
    // `cli_` prefix that distinguishes CLI-imported sessions from
    // KAS-native ones (which use `sess_`).
    const newId = basename(importedDir);
    expect(newId).not.toBe('sess_roundtrip-1');
    expect(newId).toMatch(/^cli_[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);

    // 4. Validate session.json against SessionMetadataSchema and confirm
    //    every field the Rust import path rewrites OR preserves.
    const meta = JSON.parse(readFileSync(join(importedDir, 'session.json'), 'utf8'));
    const metaParse = SessionMetadataSchema.safeParse(meta);
    expect(metaParse.success).toBe(true);
    if (!metaParse.success) {
      throw new Error('session.json failed schema: ' + JSON.stringify(metaParse.error.format()));
    }
    // Rewritten on import - returned path basename should equal the
    // imported session id stamped in session.json.
    expect(metaParse.data.id).toBe(newId);
    expect(metaParse.data.workspacePaths).toEqual([newCwd]);
    expect(metaParse.data.schemaVersion).toBe('1.0.0');
    expect(new Date(metaParse.data.lastModifiedAt).getTime()).toBeGreaterThanOrEqual(
      new Date(metaParse.data.createdAt).getTime(),
    );
    // Preserved verbatim from the seeded session (the captured fixture's
    // dataModelVersion and agentMode come along, the test-supplied title
    // is preserved, and createdAt is NOT bumped).
    expect(metaParse.data.title).toBe('roundtrip fixture');
    expect(metaParse.data.agentMode).toBe('vibe');
    expect(meta.dataModelVersion).toBe(1);

    // 5. Validate every line of messages.jsonl against PersistedMessageSchema.
    const lines = readJsonl(join(importedDir, 'messages.jsonl'));
    expect(lines.length).toBe(FIXTURE_MESSAGES.length);
    const parsedTypes = new Set<string>();
    for (const [i, line] of lines.entries()) {
      const r = PersistedMessageSchema.safeParse(line);
      expect(r.success).toBe(true);
      if (!r.success) {
        throw new Error(
          `messages.jsonl line ${i} failed schema: ` + JSON.stringify(r.error.format()),
        );
      }
      parsedTypes.add(r.data.payload.type);
    }
    // The fixture must cover the core round-trip variants. If a refresh
    // produces a fixture missing any of these, re-run the prompt or extend
    // it - silently dropping coverage on `tool_call` round-trip would
    // defeat the purpose of using a captured session.
    for (const required of [
      'user',
      'assistant',
      'tool_call',
      'tool_result',
      'turn_start',
      'turn_end',
    ]) {
      expect(parsedTypes).toContain(required);
    }
    // Distinct-type count is not strictly required to grow, but a regression
    // here means the fixture got narrower and is worth investigating.
    expect(parsedTypes.size).toBeGreaterThanOrEqual(8);

    // Order is also preserved: messages.jsonl is byte-faithful, so the
    // first line is always the user prompt and the last is `turn_end`.
    expect((lines[0] as { payload: { type: string } }).payload.type).toBe('user');
    expect((lines[lines.length - 1] as { payload: { type: string } }).payload.type).toBe('turn_end');

    // 6. Re-export from the imported dir (round-trip stage 2) to make sure
    //    the imported artifact is itself exportable.
    const archive2 = join(tmp, 'roundtrip-2.zip');
    const exportRes2 = runCli([
      'export-session',
      '--id', newId,
      '--cwd', newCwd,
      '--out', archive2,
      '--base-path', destRoot,
    ]);
    expect(exportRes2.exitCode).toBe(0);
    expect(exportRes2.parsed.success).toBe(true);
    expect(existsSync(archive2)).toBe(true);
  });

  it('--force overwrites existing output; without it, export errors', () => {
    const sessionsRoot = join(tmp, 'sessions');
    seedSession({
      basePath: sessionsRoot,
      workspacePath: cwd,
      sessionId: 'sess_force-test',
    });
    const out = join(tmp, 'taken.zip');
    writeFileSync(out, 'pre-existing content');

    const noForce = runCli([
      'export-session',
      '--id', 'sess_force-test',
      '--cwd', cwd,
      '--out', out,
      '--base-path', sessionsRoot,
    ]);
    expect(noForce.exitCode).toBe(1);
    expect(noForce.parsed.success).toBe(false);
    // Pre-existing file untouched.
    expect(readFileSync(out, 'utf8')).toBe('pre-existing content');

    const withForce = runCli([
      'export-session',
      '--id', 'sess_force-test',
      '--cwd', cwd,
      '--out', out,
      '--base-path', sessionsRoot,
      '--force',
    ]);
    expect(withForce.exitCode).toBe(0);
    expect(withForce.parsed.success).toBe(true);
    expect(statSync(out).size).toBeGreaterThan('pre-existing content'.length);
  });

  it('imported session lands under the cwd-derived bucket, not the source bucket', () => {
    const sourceRoot = join(tmp, 'src-sessions');
    seedSession({
      basePath: sourceRoot,
      workspacePath: cwd,
      sessionId: 'sess_bucket-test',
    });
    const archive = join(tmp, 'bucket.zip');
    runCli([
      'export-session',
      '--id', 'sess_bucket-test',
      '--cwd', cwd,
      '--out', archive,
      '--base-path', sourceRoot,
    ]);

    const destRoot = join(tmp, 'dst-sessions');
    const otherCwd = join(tmp, 'other-cwd');
    mkdirSync(otherCwd, { recursive: true });
    const res = runCli([
      'import-session',
      '--archive', archive,
      '--cwd', otherCwd,
      '--base-path', destRoot,
    ]);
    expect(res.exitCode).toBe(0);

    // ONLY the destination bucket for `otherCwd` should exist - not one for `cwd`.
    const sourceBucket = computeWorkspaceHash([cwd]);
    const destBucket = computeWorkspaceHash([otherCwd]);
    expect(sourceBucket).not.toBe(destBucket); // sanity: distinct cwds -> distinct buckets
    const buckets = readdirSync(destRoot);
    expect(buckets).toContain(destBucket);
    expect(buckets).not.toContain(sourceBucket);
  });

  it('import strips parentSessionId / parentExecutionId / lastCheckpointId', () => {
    // Seed a session that includes the linkage fields KAS populates for
    // sub-execution sessions. Importing it as a top-level archive must
    // null them out - they reference IDs from the source environment
    // and would dangle in the destination.
    const sourceRoot = join(tmp, 'src-sessions');
    const seededDir = seedSession({
      basePath: sourceRoot,
      workspacePath: cwd,
      sessionId: 'sess_with-parents',
    });
    const sessionJsonPath = join(seededDir, 'session.json');
    // Type the seeded metadata against the canonical schema so that if KAS
    // ever removes one of these field names from SessionMetadata, this
    // injection step fails at compile time instead of silently testing
    // nothing.
    const seeded: SessionMetadata = SessionMetadataSchema.parse(
      JSON.parse(readFileSync(sessionJsonPath, 'utf8')),
    );
    seeded.parentSessionId = 'sess_parent-source';
    seeded.parentExecutionId = 'exec_parent-source';
    seeded.lastCheckpointId = 'checkpoint-source';
    writeFileSync(sessionJsonPath, JSON.stringify(seeded, null, 2));

    const archive = join(tmp, 'with-parents.zip');
    const exportRes = runCli([
      'export-session',
      '--id', 'sess_with-parents',
      '--cwd', cwd,
      '--out', archive,
      '--base-path', sourceRoot,
    ]);
    expect(exportRes.exitCode).toBe(0);

    const destRoot = join(tmp, 'dst-sessions');
    const importRes = runCli([
      'import-session',
      '--archive', archive,
      '--cwd', cwd,
      '--base-path', destRoot,
    ]);
    expect(importRes.exitCode).toBe(0);

    const importedMeta: SessionMetadata = SessionMetadataSchema.parse(
      JSON.parse(readFileSync(join(importRes.parsed.path!, 'session.json'), 'utf8')),
    );
    expect(importedMeta.parentSessionId).toBeUndefined();
    expect(importedMeta.parentExecutionId).toBeUndefined();
    expect(importedMeta.lastCheckpointId).toBeUndefined();
  });
});
