/**
 * Wrapper around the hidden `kiro-cli chat _ ensure-session`
 * subcommand. The TUI's resume paths (`--resume`, `--resume-id`,
 * `/chat` picker) shell out to this before issuing `session/load`
 * so V2 ids resolve into KAS storage when the active engine is KAS.
 *
 * Argv assembly only - the spawn-and-parse contract lives in
 * `chat-internal-cli.ts`.
 */

import {
  type AsyncSpawner,
  type RunResult,
  runChatInternalAsync,
} from './chat-internal-cli';
import type { AgentEngine } from '../agent-engine';

export type { AsyncSpawner };

export type SourceFormat = 'auto' | 'classic' | 'v2' | 'kas';
export type TargetFormat = AgentEngine;

export interface EnsureSessionInput {
  /** Source session id. V2 UUID, KAS `sess_<uuid>`, etc. Required. */
  sourceSessionId: string;
  /**
   * Source format. `auto` triggers a lookup across all non-target
   * formats; explicit variants skip the lookup.
   */
  sourceFormat: SourceFormat;
  /** Target engine. The session will be loadable in this engine afterwards. */
  targetFormat: TargetFormat;
  /** Workspace path. Required - selects the workspace-hashed bucket for KAS. */
  cwd: string;
}

export type EnsureSessionResult = RunResult<{ sessionId: string }>;

/**
 * Shell out to `chat _ ensure-session`. On success returns the
 * target-engine-native session id.
 */
export async function ensureSession(
  input: EnsureSessionInput,
  spawner?: AsyncSpawner
): Promise<EnsureSessionResult> {
  const args = [
    'chat',
    '_',
    'ensure-session',
    '--source-format',
    input.sourceFormat,
    '--source-session-id',
    input.sourceSessionId,
    '--target-format',
    input.targetFormat,
    '--cwd',
    input.cwd,
  ];
  return runChatInternalAsync(
    args,
    (parsed) =>
      typeof parsed.sessionId === 'string'
        ? { sessionId: parsed.sessionId }
        : null,
    spawner
  );
}
