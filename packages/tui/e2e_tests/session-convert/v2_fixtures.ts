/**
 * V2 session fixtures used by tests in this directory.
 *
 * Each fixture is a `session.json` + `messages.jsonl` pair captured
 * from a real `kiro-cli chat --agent-engine v2` run. Tests reference
 * the fixture's UUID via the `sessionId` const; the fixture files
 * themselves are read by tests that need to feed the V2 -> KAS
 * converter input shape.
 *
 * UUIDs are duplicated as consts rather than parsed from the fixture
 * at runtime so a drift between fixture and test fails loudly.
 *
 * KAS fixtures live in a separate `kas_fixtures.ts` module.
 */
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

const withCompactionLog = readFileSync(
  resolve(
    __dirname,
    '../../../../crates/chat-cli-v2/src/agent/kas/fixtures/v2/with_compaction/messages.jsonl'
  ),
  'utf8'
);

/** Pull the `Compaction` entry's `summary` out of a V2 message log. */
function compactionSummary(messagesJsonl: string): string {
  for (const line of messagesJsonl.trim().split('\n')) {
    const entry = JSON.parse(line);
    if (entry.kind === 'Compaction') return entry.data.summary as string;
  }
  throw new Error('fixture log has no Compaction entry');
}

export interface V2Fixture {
  /**
   * Human label for the fixture, used in test descriptions
   * (`it("converts the V2 ${name} fixture ...")`). Not derived from
   * the disk path so it can stay stable when `fixtureDir` moves.
   */
  name: string;
  /** Absolute path to the directory containing the fixture files. */
  fixtureDir: string;
  /** V2 UUID present in `session.json` as `session_id`. */
  sessionId: string;
}

/**
 * Linear V2 session exercising `FsRead`, `FsWrite`, `ExecuteCmd`,
 * `Grep`, and `Glob` in a single conversation. No subagent, no
 * todo_list. See `fixtures/v2/basic_fs_tools/README.md` for the
 * original prompt and refresh instructions.
 */
export const BASIC_FS_TOOLS = {
  name: 'basic_fs_tools',
  fixtureDir: resolve(
    __dirname,
    '../../../../crates/chat-cli-v2/src/agent/kas/fixtures/v2/basic_fs_tools'
  ),
  sessionId: '8e247382-c779-440f-b1db-ed9bad1afaee',
  /**
   * Absolute path the original capture wrote tool inputs against.
   * Used only to assert the converted tool_call args are preserved
   * verbatim from the V2 source - NOT for KAS bucketing, which
   * depends on the runtime cwd.
   */
  capturedToolInputCwd:
    '/private/var/folders/zn/6bkhzvmx4lvf3mgr7qqr2x940000gq/T/kiro-fixture-fs-tools-XXXX.W9fNVpVZqh',
  /**
   * V2 `Prompt` `message_id` for the (only) user prompt in the
   * fixture. Distinct from `sessionId`. Tests use this to assert
   * the converter copies it onto the KAS `user` payload's `id`
   * field unchanged.
   */
  userPromptId: '8542d19b-c7b1-4b73-8d51-e33d2f53e1b5',
  /**
   * Tool uses present in the V2 fixture, in the order the
   * `AssistantMessage` entries appear. Pins the converter's
   * output ordering and tool name preservation.
   */
  toolUses: [
    { id: 'tooluse_gFrggjV9mXidPDS17K1ITK', name: 'write' },
    { id: 'tooluse_snTlZqKfPN0hk8qRb1CeLB', name: 'read' },
    { id: 'tooluse_cSd8AEH4CNqdO9BaFCQQ4X', name: 'write' },
    { id: 'tooluse_jEX5aynOjMoxurQEYWFXGw', name: 'shell' },
    { id: 'tooluse_Z6Y0sa9zMD1VTl1vXaJDvs', name: 'grep' },
    { id: 'tooluse_DBNDfgv8VsfjKCLEgN9PgX', name: 'glob' },
  ] as const,
} satisfies V2Fixture & {
  capturedToolInputCwd: string;
  userPromptId: string;
  toolUses: readonly { readonly id: string; readonly name: string }[];
};

/**
 * V2 session that exercises a `/compact` mid-conversation: basic
 * `write`/`read` turns, a `LogEntry::Compaction`, then one more turn
 * that runs after the compaction. Drives the V2 `Compaction` -> KAS
 * `summarization` mapping. See `fixtures/v2/with_compaction/README.md`
 * for the capture procedure.
 */
export const WITH_COMPACTION = {
  name: 'with_compaction',
  fixtureDir: resolve(
    __dirname,
    '../../../../crates/chat-cli-v2/src/agent/kas/fixtures/v2/with_compaction'
  ),
  sessionId: '5912f694-10d5-4b04-ad6e-e2cc4816cbc7',
  /** `message_id` of the first user prompt (before the compaction). */
  userPromptId: 'd087bafe-515c-4eba-9867-0a0dc61e9ccc',
  /** `message_id` of the prompt that runs AFTER the `/compact`. */
  postCompactionPromptId: '890c26fe-702a-4fbd-ad86-08cba56728b7',
  /** Exact `summary` text stored on the fixture's `Compaction` entry. */
  summary: compactionSummary(withCompactionLog),
  /** Raw `messages.jsonl` text, for prompt-id -> user-payload mapping. */
  messagesJsonl: withCompactionLog,
} satisfies V2Fixture & {
  userPromptId: string;
  postCompactionPromptId: string;
  summary: string;
  messagesJsonl: string;
};

/**
 * Options for [`seedV2Session`].
 */
export interface SeedV2SessionOptions {
  /**
   * Directory to write the seeded files into. The caller decides
   * whether this is a flat sessions root (`<v2-sessions>/<id>.json`)
   * or a nested `<kiroHome>/sessions/cli/` layout. The directory is
   * created if missing.
   */
  rootDir: string;
  /** V2 UUID used as the basename of the seeded files. */
  sessionId: string;
  /** Fixture directory holding the source `session.json` + `messages.jsonl`. */
  fixtureDir: string;
  /**
   * Value to write into the seeded `session.json`'s `cwd` field. V2's
   * `list_sessions_impl` filters by canonical `cwd` equality, so tests
   * that exercise the picker / `session/list` path need this set to
   * the runtime cwd they will pass to the binary; tests that look up
   * sessions by id directly can pass any value. The fixture file on
   * disk is not modified.
   */
  cwd: string;
}

/**
 * Copy a V2 fixture into a sessions directory as `<id>.json` +
 * `<id>.jsonl`, rewriting the seeded `session.json`'s `cwd` field to
 * `opts.cwd`. The source fixture file is left untouched.
 *
 * Returns the absolute paths of the two seeded files so callers can
 * assert on them or do further per-test mutation if needed.
 */
export function seedV2Session(opts: SeedV2SessionOptions): {
  metadataPath: string;
  logPath: string;
} {
  mkdirSync(opts.rootDir, { recursive: true });
  const metadataPath = join(opts.rootDir, `${opts.sessionId}.json`);
  const logPath = join(opts.rootDir, `${opts.sessionId}.jsonl`);

  const sourceMetadata = readFileSync(
    join(opts.fixtureDir, 'session.json'),
    'utf8'
  );
  const parsed = JSON.parse(sourceMetadata) as Record<string, unknown>;
  parsed.cwd = opts.cwd;
  writeFileSync(metadataPath, JSON.stringify(parsed, null, 2));

  copyFileSync(join(opts.fixtureDir, 'messages.jsonl'), logPath);

  return { metadataPath, logPath };
}
