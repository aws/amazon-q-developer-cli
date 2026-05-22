/**
 * Detection of spec-artifact paths in tool call arguments.
 *
 * A spec artifact path is one of:
 *   .kiro/specs/<feature>/requirements.md
 *   .kiro/specs/<feature>/design.md
 *   .kiro/specs/<feature>/tasks.md
 *
 * `<feature>` must not contain a path separator (single directory level).
 * The match is permissive at the start of the path so it works on absolute
 * paths emitted by the agent (`/Users/.../workspace/.kiro/specs/...`) and
 * on workspace-relative paths.
 */

import { resolve, isAbsolute } from 'node:path';
import type { ArtifactKind } from './spec-artifact-loader.js';

/** Match anywhere in the path, not just from the start. */
const SPEC_ARTIFACT_RE =
  /(?:^|[\\/])\.kiro[\\/]specs[\\/]([^\\/]+)[\\/](requirements|design|tasks)\.md$/;

export interface SpecArtifactPathMatch {
  /** Original path as provided in the tool call args. */
  originalPath: string;
  /** Absolute path resolved against `workspaceRoot`. Used as the dedup key. */
  absolutePath: string;
  featureName: string;
  artifact: ArtifactKind;
}

/**
 * If `path` looks like a spec artifact path, return the structured match;
 * otherwise return null. Pure / synchronous — does not touch the filesystem.
 */
export function matchSpecArtifactPath(
  path: string,
  workspaceRoot: string
): SpecArtifactPathMatch | null {
  if (typeof path !== 'string' || path.length === 0) return null;
  const m = SPEC_ARTIFACT_RE.exec(path);
  if (!m) return null;
  const featureName = m[1]!;
  const artifact = m[2] as ArtifactKind;
  const absolutePath = isAbsolute(path) ? path : resolve(workspaceRoot, path);
  return {
    originalPath: path,
    absolutePath,
    featureName,
    artifact,
  };
}

/**
 * Extract the `path` argument from a tool call's args object. The agent
 * uses `path` for both `fs_write` (V1 alias) and `Write` (KAS native).
 * Returns null when the args don't contain a string `path`.
 */
export function extractToolPath(
  args: Record<string, unknown> | undefined
): string | null {
  if (!args) return null;
  const p = args['path'];
  return typeof p === 'string' ? p : null;
}

/**
 * Recognised tool names that may write a file. We accept multiple
 * variants so we stay compatible across agent engines:
 *
 *   - `fs_write` — V1 alias used historically.
 *   - `Write`    — KAS native single-purpose write tool.
 *   - `create`   — KAS spec workflow uses this lowercase variant
 *                  with `args.command === 'create'` for new files.
 *   - `Edit`/`fs_edit` — multiplex tools whose operation is given by
 *                  `args.command` (e.g. `create`, `update`, `replace`).
 *                  The caller must check `command` before treating it
 *                  as a write; we handle that in `isWriteOperation`.
 */
const KNOWN_WRITE_TOOL_NAMES = new Set([
  'fs_write',
  'Write',
  'create',
  'Edit',
  'fs_edit',
]);

/**
 * Operation strings that imply the tool is producing/modifying file
 * content (and therefore worth tracking as a generation event).
 *
 * The first four (`create`, `str_replace`, `insert`, `append`) are the
 * canonical values for V1's `fs_write` tool — see the `FsWrite` enum
 * in `crates/chat-cli/src/cli/chat/tools/fs_write.rs`. They MUST stay
 * in sync with the Rust enum's `#[serde(rename = ...)]` attributes.
 *
 * The remainder (`update`, `replace`, `write`, `overwrite`) are
 * conjectural names that some KAS multiplex tools may emit; they're
 * accepted defensively. None map to a known wire format today.
 */
const WRITE_COMMAND_VALUES = new Set([
  // V1 `fs_write` canonical commands.
  'create',
  'str_replace',
  'insert',
  'append',
  // Defensive: possible KAS multiplex command names.
  'update',
  'replace',
  'write',
  'overwrite',
]);

/**
 * True for tool names that the agent uses to write file content. We
 * include the V1 alias (`fs_write`), the KAS native names (`Write`,
 * `create`), and the multiplex `Edit` / `fs_edit` tools whose
 * operation is selected via `args.command`.
 *
 * Some tools in this set (`Edit` / `fs_edit`) handle non-write
 * operations too; the caller must combine this check with
 * `isWriteOperation(args)` to decide whether the tool call is
 * actually creating or replacing file content.
 */
export function isFileWriteToolName(name: string): boolean {
  return KNOWN_WRITE_TOOL_NAMES.has(name);
}

/**
 * True when the tool args either don't carry a `command` field (the
 * single-purpose write tools `fs_write` / `Write` / `create`) or carry
 * one of the recognised write operations. False when `command` is
 * present but names a non-write op (e.g. `delete`, `move`).
 *
 * Pure / synchronous: relies only on the args object.
 */
export function isWriteOperation(
  args: Record<string, unknown> | undefined
): boolean {
  if (!args) return true;
  const cmd = args['command'];
  if (cmd === undefined || cmd === null) return true;
  if (typeof cmd !== 'string') return false;
  return WRITE_COMMAND_VALUES.has(cmd);
}
