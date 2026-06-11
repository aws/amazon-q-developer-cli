/**
 * Wrappers around the hidden `kiro-cli chat _ export-session` and
 * `kiro-cli chat _ import-session` subcommands.
 *
 * These subcommands are the canonical surface for KAS session archive
 * operations - the algorithm lives in `crates/chat-cli-v2/src/agent/kas/`.
 * The TUI's `/chat save` and `/chat load` slash command handlers route
 * through here. The hidden `chat _` namespace is intentionally not in
 * `--help` so end users can't invoke it directly.
 *
 * Argv assembly only - the spawn-and-parse contract lives in
 * `chat-internal-cli.ts`.
 */

import type { ErrorCode } from '../types/generated/chat-internal';
import { type SyncSpawner, runChatInternalSync } from './chat-internal-cli';

export type { SyncSpawner };

export interface ExportSessionInput {
  /** KAS session id to export. Required. */
  sessionId: string;
  /** Workspace path. Required - selects which workspace-hashed bucket to read from. */
  cwd: string;
  /** Output zip path. Required. */
  out: string;
  /** Sessions root override. Defaults to `$KIRO_HOME/.kiro/sessions`. */
  basePath?: string;
  /** Overwrite existing output file. */
  force?: boolean;
}

export interface ImportSessionInput {
  /** Absolute or expandable archive path to import. Required. */
  archivePath: string;
  /** Workspace path. Required - selects which workspace-hashed bucket to write to. */
  cwd: string;
  /** Sessions root override. Defaults to `$KIRO_HOME/.kiro/sessions`. */
  basePath?: string;
}

export type ArchiveResult =
  | { ok: true; path: string }
  | { ok: false; message: string; code?: ErrorCode };

function archiveResultFor(
  expectedKind: 'exportSession' | 'importSession',
  args: string[],
  spawner?: SyncSpawner
): ArchiveResult {
  const r = runChatInternalSync(args, spawner);
  if (!r.ok) return { ok: false, message: r.message };
  if (r.output.kind === 'error') {
    return {
      ok: false,
      message: r.output.data.message,
      ...(r.output.data.code ? { code: r.output.data.code } : {}),
    };
  }
  if (r.output.kind !== expectedKind) {
    return {
      ok: false,
      message: `Unexpected response kind from kiro-cli: ${r.output.kind}`,
    };
  }
  return { ok: true, path: r.output.data.path };
}

export function exportSession(
  input: ExportSessionInput,
  spawner?: SyncSpawner
): ArchiveResult {
  const args = [
    'chat',
    '_',
    'export-session',
    '--id',
    input.sessionId,
    '--cwd',
    input.cwd,
    '--out',
    input.out,
  ];
  if (input.basePath) args.push('--base-path', input.basePath);
  if (input.force) args.push('--force');
  return archiveResultFor('exportSession', args, spawner);
}

export function importSession(
  input: ImportSessionInput,
  spawner?: SyncSpawner
): ArchiveResult {
  const args = [
    'chat',
    '_',
    'import-session',
    '--archive',
    input.archivePath,
    '--cwd',
    input.cwd,
  ];
  if (input.basePath) args.push('--base-path', input.basePath);
  return archiveResultFor('importSession', args, spawner);
}
