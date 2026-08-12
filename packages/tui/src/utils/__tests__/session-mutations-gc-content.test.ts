import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  rmSync,
  existsSync,
  utimesSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { gcScan, gcEmptySessions } from '../session-mutations';

let root: string;

/** A KAS session dir, optionally with a transcript, backdated past the guard. */
function kasSession(
  id: string,
  opts: { metadata?: unknown; transcript?: string; aged?: boolean } = {}
): string {
  const dir = join(root, 'hash1', `sess_${id}`);
  mkdirSync(dir, { recursive: true });
  const metaPath = join(dir, 'session.json');
  writeFileSync(
    metaPath,
    JSON.stringify(opts.metadata ?? { id: `sess_${id}`, title: 'New Session' })
  );
  const logPath = join(dir, 'messages.jsonl');
  writeFileSync(logPath, opts.transcript ?? '');
  if (opts.aged) {
    const past = new Date(Date.now() - 2 * 60 * 60 * 1000);
    utimesSync(metaPath, past, past);
    utimesSync(logPath, past, past);
  }
  return dir;
}

const TRANSCRIPT =
  JSON.stringify({ type: 'turn_start', executionId: 'e1' }) +
  '\n' +
  JSON.stringify({ type: 'user', content: 'real work worth keeping' }) +
  '\n';

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'gc-content-'));
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

describe('gc KAS emptiness is content-based, not title-based', () => {
  // The old check read only `metadata.title`. CLI-created sessions persist
  // `metadata: {}` while holding a full transcript, so real conversations
  // were classified as empty and deleted.
  it('keeps an untitled session that has a transcript', async () => {
    const dir = kasSession('keep', {
      metadata: {},
      transcript: TRANSCRIPT,
      aged: true,
    });

    const scan = await gcScan(null, new Set(), root);

    // Ids are normalized (sess_ stripped) in scan candidates.
    expect(scan.candidates.map((c) => c.sessionId)).not.toContain('keep');
    expect(existsSync(dir)).toBe(true);
  });

  it('keeps a placeholder-titled session that has a transcript', async () => {
    kasSession('titled_husk_with_content', {
      metadata: { id: 'sess_x', title: 'New Session' },
      transcript: TRANSCRIPT,
      aged: true,
    });

    const scan = await gcScan(null, new Set(), root);

    expect(scan.candidates).toHaveLength(0);
  });

  it('still collects a genuinely empty session', async () => {
    kasSession('husk', { aged: true });

    const scan = await gcScan(null, new Set(), root);

    expect(scan.candidates.map((c) => c.sessionId)).toContain('husk');
  });
});

describe('gc KAS recency guard tracks transcript activity', () => {
  // A turn appends to messages.jsonl without rewriting session.json, so
  // metadata mtime alone can present an active session as long-abandoned.
  it('treats fresh transcript activity as live despite stale metadata', async () => {
    const dir = join(root, 'hash1', 'sess_live');
    mkdirSync(dir, { recursive: true });
    const metaPath = join(dir, 'session.json');
    writeFileSync(
      metaPath,
      JSON.stringify({ id: 'sess_live', title: 'New Session' })
    );
    writeFileSync(join(dir, 'messages.jsonl'), ''); // empty, but touched now
    const past = new Date(Date.now() - 48 * 60 * 60 * 1000);
    utimesSync(metaPath, past, past);

    const scan = await gcScan(null, new Set(), root);

    expect(scan.skipped.recent).toBe(1);
    expect(scan.candidates).toHaveLength(0);
  });
});

describe('gcEmptySessions re-verifies emptiness at deletion time', () => {
  it('skips a candidate that gained content after the scan', async () => {
    const dir = kasSession('race', { aged: true });
    const scan = await gcScan(null, new Set(), root);
    expect(scan.candidates).toHaveLength(1);

    // The user prompts this session while the confirmation banner is up.
    writeFileSync(join(dir, 'messages.jsonl'), TRANSCRIPT);

    const result = await gcEmptySessions(scan.candidates, null, root);

    expect(result.deleted).toBe(0);
    expect(result.stale).toBe(1);
    expect(existsSync(dir)).toBe(true);
  });

  it('deletes a candidate that is still empty', async () => {
    const dir = kasSession('gone', { aged: true });
    const scan = await gcScan(null, new Set(), root);

    const result = await gcEmptySessions(scan.candidates, null, root);

    expect(result.deleted).toBe(1);
    expect(result.stale).toBe(0);
    expect(existsSync(dir)).toBe(false);
  });
});
