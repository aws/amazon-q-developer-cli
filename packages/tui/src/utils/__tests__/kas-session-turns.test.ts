import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import {
  mkdtempSync,
  mkdirSync,
  writeFileSync,
  rmSync,
  symlinkSync,
} from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { buildKasTurnTree, buildV2TurnList } from '../kas-session-turns';
import { findKasSessionDir, findKasSessionDirs } from '../session-store';

function makeSessionDir(): string {
  const root = mkdtempSync(join(tmpdir(), 'kas-turns-'));
  const dir = join(root, 'hash1', 'sess_abc');
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, 'session.json'), JSON.stringify({ id: 'sess_abc' }));
  return dir;
}

function writeMessages(
  dir: string,
  events: Array<Record<string, unknown>>
): void {
  const lines = events.map((payload) =>
    JSON.stringify({ id: Math.random().toString(36), payload })
  );
  writeFileSync(join(dir, 'messages.jsonl'), lines.join('\n') + '\n');
}

function writeSubExecution(
  dir: string,
  subId: string,
  events: Array<Record<string, unknown>>
): void {
  const subDir = join(dir, 'sub-executions');
  mkdirSync(subDir, { recursive: true });
  const lines = events.map((payload) =>
    JSON.stringify({ id: Math.random().toString(36), payload })
  );
  writeFileSync(join(subDir, `${subId}.jsonl`), lines.join('\n') + '\n');
}

describe('buildKasTurnTree', () => {
  let dir: string;

  beforeEach(() => {
    dir = makeSessionDir();
  });

  afterEach(() => {
    rmSync(join(dir, '..', '..'), { recursive: true, force: true });
  });

  it('attaches a pre-turn user prompt (no executionId) to the next turn', () => {
    // Real KAS shape: `user` prompt emitted before `turn_start`, carrying
    // no executionId; the turn's id arrives on turn_start.
    writeMessages(dir, [
      { type: 'user', content: 'review the codebase' },
      { type: 'turn_start', executionId: 'e1' },
      { type: 'assistant', executionId: 'e1', content: 'on it' },
      { type: 'tool_call', executionId: 'e1', toolName: 'code' },
    ]);

    const turns = buildKasTurnTree(dir);
    expect(turns).toHaveLength(1);
    expect(turns[0]!.userText).toBe('review the codebase');
    expect(turns[0]!.assistantText).toBe('on it');
    expect(turns[0]!.toolNames).toEqual(['code']);
  });

  it('groups events into turns by executionId', () => {
    writeMessages(dir, [
      { type: 'user', text: 'first question' },
      { type: 'turn_start', executionId: 'e1' },
      { type: 'assistant', executionId: 'e1', text: 'first answer' },
      { type: 'user', text: 'second question' },
      { type: 'turn_start', executionId: 'e2' },
      { type: 'assistant', executionId: 'e2', text: 'second answer' },
    ]);

    const turns = buildKasTurnTree(dir);
    expect(turns).toHaveLength(2);
    expect(turns[0]!.executionId).toBe('e1');
    expect(turns[0]!.userText).toBe('first question');
    expect(turns[0]!.assistantText).toBe('first answer');
    expect(turns[1]!.userText).toBe('second question');
  });

  it('collects tool names per turn', () => {
    writeMessages(dir, [
      { type: 'user', text: 'do stuff' },
      { type: 'turn_start', executionId: 'e1' },
      { type: 'tool_call', executionId: 'e1', toolName: 'fs_write' },
      { type: 'tool_call', executionId: 'e1', toolName: 'execute_bash' },
      { type: 'tool_call', executionId: 'e1', toolName: 'fs_write' },
    ]);

    const turns = buildKasTurnTree(dir);
    expect(turns[0]!.toolNames).toEqual(['fs_write', 'execute_bash']);
  });

  it('nests subagents under the spawning turn', () => {
    writeMessages(dir, [
      { type: 'user', text: 'review the code' },
      { type: 'turn_start', executionId: 'e1' },
      {
        type: 'tool_call',
        executionId: 'e1',
        toolName: 'orchestrate_subagent',
      },
    ]);
    writeSubExecution(dir, 'sub1', [
      {
        type: 'assistant',
        executionId: 'e1',
        subExecutionId: 'sub1',
        text: 'reviewing auth',
      },
      {
        type: 'tool_call',
        executionId: 'e1',
        subExecutionId: 'sub1',
        toolName: 'fs_read',
      },
    ]);
    writeSubExecution(dir, 'sub2', [
      {
        type: 'assistant',
        executionId: 'e1',
        subExecutionId: 'sub2',
        text: 'reviewing db',
      },
    ]);

    const turns = buildKasTurnTree(dir);
    expect(turns).toHaveLength(1);
    expect(turns[0]!.subagents).toHaveLength(2);
    const s1 = turns[0]!.subagents.find((s) => s.subExecutionId === 'sub1')!;
    expect(s1.toolCallCount).toBe(1);
    expect(s1.summary).toBe('reviewing auth');
  });

  it('caps the number of sub-execution files read for one preview', () => {
    writeMessages(dir, [
      { type: 'user', text: 'fan out' },
      { type: 'turn_start', executionId: 'e1' },
    ]);
    for (let index = 0; index < 65; index += 1) {
      const subId = `sub-${String(index).padStart(2, '0')}`;
      writeSubExecution(dir, subId, [
        {
          type: 'assistant',
          executionId: 'e1',
          subExecutionId: subId,
          text: `worker ${index}`,
        },
      ]);
    }

    const turns = buildKasTurnTree(dir);

    expect(turns[0]!.subagents).toHaveLength(64);
  });

  it('handles a session with no subagents', () => {
    writeMessages(dir, [
      { type: 'user', text: 'hi' },
      { type: 'turn_start', executionId: 'e1' },
      { type: 'assistant', executionId: 'e1', text: 'hello' },
    ]);

    const turns = buildKasTurnTree(dir);
    expect(turns[0]!.subagents).toHaveLength(0);
    expect(turns.reduce((n, t) => n + t.subagents.length, 0)).toBe(0);
  });

  it('skips malformed lines', () => {
    writeFileSync(
      join(dir, 'messages.jsonl'),
      JSON.stringify({ payload: { type: 'user', text: 'valid' } }) +
        '\nnot json\n' +
        JSON.stringify({ payload: { type: 'turn_start', executionId: 'e1' } }) +
        '\n' +
        JSON.stringify({
          payload: { type: 'assistant', executionId: 'e1', text: 'answer' },
        }) +
        '\n'
    );

    const turns = buildKasTurnTree(dir);
    expect(turns).toHaveLength(1);
    expect(turns[0]!.userText).toBe('valid');
    expect(turns[0]!.assistantText).toBe('answer');
  });

  it.skipIf(process.platform === 'win32')(
    'rejects a symlinked main transcript',
    () => {
      const outside = join(dir, '..', '..', 'outside.jsonl');
      writeFileSync(
        outside,
        `${JSON.stringify({ payload: { type: 'turn_start', executionId: 'outside' } })}\n`
      );
      rmSync(join(dir, 'messages.jsonl'), { force: true });
      symlinkSync(outside, join(dir, 'messages.jsonl'));

      expect(buildKasTurnTree(dir)).toEqual([]);
    }
  );

  it('does not create spurious turns from non-turn executionIds (approvals)', () => {
    // Real KAS: approval interactions carry their OWN executionId (tool-call
    // ids like `toolu_…`/`run_command…`) that must NOT become empty turns.
    writeMessages(dir, [
      { type: 'user', text: 'do the thing' },
      { type: 'turn_start', executionId: 'e1' },
      { type: 'tool_call', executionId: 'e1', toolName: 'execute_bash' },
      { type: 'pending_interaction', executionId: 'run_command_abc' },
      { type: 'interaction_resolved', executionId: 'run_command_abc' },
      { type: 'pending_interaction', executionId: 'toolu_xyz' },
      { type: 'interaction_resolved', executionId: 'toolu_xyz' },
      { type: 'assistant', executionId: 'e1', text: 'done' },
    ]);

    const turns = buildKasTurnTree(dir);
    // Exactly ONE real turn — the approval interaction ids are ignored.
    expect(turns).toHaveLength(1);
    expect(turns[0]!.userText).toBe('do the thing');
    expect(turns[0]!.assistantText).toBe('done');
    expect(turns.some((t) => t.userText === '')).toBe(false);
  });

  it('returns empty for a session with no messages', () => {
    expect(buildKasTurnTree(dir)).toHaveLength(0);
  });

  it('extracts text from content-block arrays', () => {
    writeMessages(dir, [
      { type: 'user', content: [{ text: 'block one' }, { text: 'block two' }] },
      { type: 'turn_start', executionId: 'e1' },
    ]);

    const turns = buildKasTurnTree(dir);
    expect(turns[0]!.userText).toBe('block one block two');
  });

  it('ignores a subagent whose parent turn is absent', () => {
    writeMessages(dir, [
      { type: 'user', text: 'hi' },
      { type: 'turn_start', executionId: 'e1' },
    ]);
    writeSubExecution(dir, 'orphan', [
      {
        type: 'assistant',
        executionId: 'e-missing',
        subExecutionId: 'orphan',
        text: 'x',
      },
    ]);

    const turns = buildKasTurnTree(dir);
    expect(turns[0]!.subagents).toHaveLength(0);
  });

  it('counts total subagents across turns', () => {
    writeMessages(dir, [
      { type: 'user', text: 'a' },
      { type: 'turn_start', executionId: 'e1' },
      { type: 'user', text: 'b' },
      { type: 'turn_start', executionId: 'e2' },
    ]);
    writeSubExecution(dir, 's1', [
      { type: 'assistant', executionId: 'e1', subExecutionId: 's1', text: 'x' },
    ]);
    writeSubExecution(dir, 's2', [
      { type: 'assistant', executionId: 'e2', subExecutionId: 's2', text: 'y' },
    ]);
    writeSubExecution(dir, 's3', [
      { type: 'assistant', executionId: 'e2', subExecutionId: 's3', text: 'z' },
    ]);

    const turns = buildKasTurnTree(dir);
    expect(turns.reduce((n, t) => n + t.subagents.length, 0)).toBe(3);
  });
});

describe('findKasSessionDir', () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'kas-find-'));
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  it('finds a session dir by id across workspace hashes', () => {
    const dir = join(root, 'hashX', 'sess_target');
    mkdirSync(dir, { recursive: true });
    writeFileSync(
      join(dir, 'session.json'),
      JSON.stringify({ id: 'sess_target' })
    );

    expect(findKasSessionDir(root, 'sess_target')).toBe(dir);
    expect(findKasSessionDir(root, 'target')).toBe(dir);
  });

  it('returns null when not found', () => {
    expect(findKasSessionDir(root, 'missing')).toBeNull();
  });

  it('skips the cli (V2) store dir', () => {
    mkdirSync(join(root, 'cli', 'sess_x'), { recursive: true });
    writeFileSync(join(root, 'cli', 'sess_x', 'session.json'), '{}');
    expect(findKasSessionDir(root, 'sess_x')).toBeNull();
  });

  it('picks the canonical copy among same-id copies across hashes', () => {
    // Copy with no workspace and older activity.
    const bare = join(root, 'hashA', 'sess_dup');
    mkdirSync(bare, { recursive: true });
    writeFileSync(
      join(bare, 'session.json'),
      JSON.stringify({ id: 'sess_dup', createdAt: '2026-01-01T00:00:00.000Z' })
    );
    // Copy with a workspace — canonical resolution prefers it.
    const withWs = join(root, 'hashB', 'sess_dup');
    mkdirSync(withWs, { recursive: true });
    writeFileSync(
      join(withWs, 'session.json'),
      JSON.stringify({
        id: 'sess_dup',
        workspacePaths: ['/w/proj'],
        lastModifiedAt: '2026-06-01T00:00:00.000Z',
      })
    );

    expect(findKasSessionDir(root, 'dup')).toBe(withWs);
    expect(findKasSessionDirs(root, 'dup').sort()).toEqual(
      [bare, withWs].sort()
    );
  });

  it('locates the target without depending on unrelated sessions being readable', () => {
    const target = join(root, 'hashT', 'sess_target');
    mkdirSync(target, { recursive: true });
    writeFileSync(
      join(target, 'session.json'),
      JSON.stringify({ id: 'sess_target', workspacePaths: ['/w'] })
    );
    // An unrelated session with metadata over the read limit: a whole-store
    // scan would have to touch it, a targeted probe never does.
    const oversized = join(root, 'hashO', 'sess_other');
    mkdirSync(oversized, { recursive: true });
    writeFileSync(
      join(oversized, 'session.json'),
      JSON.stringify({ id: 'sess_other', pad: 'x'.repeat(2 * 1024 * 1024) })
    );

    expect(findKasSessionDir(root, 'target')).toBe(target);
  });
});

describe('buildV2TurnList', () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), 'v2-turns-'));
    mkdirSync(join(root, 'cli'), { recursive: true });
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  it('builds a flat turn list from Prompt/AssistantMessage events', () => {
    const lines = [
      {
        kind: 'Prompt',
        data: { content: [{ kind: 'text', data: 'first question' }] },
      },
      {
        kind: 'AssistantMessage',
        data: {
          content: [
            { kind: 'text', data: 'first answer' },
            { kind: 'tool_use', data: { name: 'fs_read' } },
          ],
        },
      },
      {
        kind: 'Prompt',
        data: { content: [{ kind: 'text', data: 'second question' }] },
      },
      {
        kind: 'AssistantMessage',
        data: { content: [{ kind: 'text', data: 'second answer' }] },
      },
    ];
    writeFileSync(
      join(root, 'cli', 's1.jsonl'),
      lines.map((l) => JSON.stringify(l)).join('\n') + '\n'
    );

    const turns = buildV2TurnList(join(root, 'cli'), 's1');
    expect(turns).toHaveLength(2);
    expect(turns[0]!.userText).toBe('first question');
    expect(turns[0]!.assistantText).toBe('first answer');
    expect(turns[0]!.toolNames).toEqual(['fs_read']);
    expect(turns[1]!.userText).toBe('second question');
    expect(turns[0]!.subagents).toEqual([]);
  });

  it('returns empty for a missing log', () => {
    expect(buildV2TurnList(join(root, 'cli'), 'nope')).toEqual([]);
  });
});
