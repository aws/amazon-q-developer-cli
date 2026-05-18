/**
 * Spec artifact loader.
 *
 * Reads `.kiro/specs/<feature>/<artifact>.md` from disk, enforces a 10 MB
 * size cap, maps fs error codes to a discriminated `LoadError` union, and
 * delegates parsing to `spec-artifact-parser`.
 *
 * The loader — not the parser — is the boundary that rejects oversize
 * files. Once bytes pass the size check the parser is guaranteed never
 * to throw and to produce deterministic output.
 */

import { existsSync } from 'node:fs';
import { open } from 'node:fs/promises';
import { join } from 'node:path';
import { specsRoot } from './spec-workspace.js';
import {
  parseArtifact,
  type ArtifactKind,
  type ArtifactSummary,
} from './spec-artifact-parser/index.js';

export type { ArtifactKind, ArtifactSummary };

/** 10 MB hard cap; files above this are rejected with `TooLarge`. */
export const ARTIFACT_MAX_BYTES = 10 * 1024 * 1024;

export type LoadError =
  | { kind: 'FeatureNotFound'; featureName: string }
  | {
      kind: 'ArtifactNotFound';
      featureName: string;
      artifact: ArtifactKind;
      path: string;
    }
  | { kind: 'TooLarge'; path: string; sizeBytes: number }
  | {
      kind: 'ReadFailed';
      path: string;
      category: 'NotFound' | 'PermissionDenied' | 'Io';
      message: string;
    };

export type LoadResult =
  | { ok: true; summary: ArtifactSummary }
  | { ok: false; error: LoadError };

/** Map a Node fs error code to our `ReadFailed.category`. */
function categorizeFsError(
  code: unknown
): 'NotFound' | 'PermissionDenied' | 'Io' {
  if (code === 'ENOENT') return 'NotFound';
  if (code === 'EACCES' || code === 'EPERM') return 'PermissionDenied';
  return 'Io';
}

/**
 * Resolve the absolute path of an artifact file under
 * `.kiro/specs/<featureName>/<artifact>.md`. Pure path math — does not
 * touch the filesystem.
 */
export function resolveArtifactPath(
  workspaceRoot: string,
  featureName: string,
  artifact: ArtifactKind
): string {
  return join(specsRoot(workspaceRoot), featureName, `${artifact}.md`);
}

/**
 * Read up to `ARTIFACT_MAX_BYTES + 1` from a file. If a 1-byte overshoot
 * succeeds we know the file is too large and can reject without buffering
 * 10+ MB. Returns `{ tooLarge: true, sizeBytes }` in that case.
 */
async function readBoundedFile(
  path: string,
  maxBytes: number
): Promise<
  { ok: true; bytes: Buffer } | { tooLarge: true; sizeBytes: number }
> {
  const fh = await open(path, 'r');
  try {
    const stat = await fh.stat();
    if (stat.size > maxBytes) {
      return { tooLarge: true, sizeBytes: stat.size };
    }
    // stat.size ≤ maxBytes — read up to that exact size.
    const buf = Buffer.alloc(stat.size);
    if (stat.size > 0) {
      await fh.read(buf, 0, stat.size, 0);
    }
    return { ok: true, bytes: buf };
  } finally {
    await fh.close();
  }
}

/**
 * Load and parse an artifact summary.
 *
 * Steps:
 *   1. Confirm the feature directory exists with a single `existsSync`
 *      check. Returns `FeatureNotFound` if missing. (Avoids the full
 *      `findSpecFeature` directory scan, which would be quadratic in
 *      the number of specs when called once per agent write event.)
 *   2. Resolve the artifact path under the feature directory.
 *      Returns `ArtifactNotFound` if the file isn't on disk.
 *   3. Read up to 10 MB; reject `TooLarge` for anything bigger.
 *   4. Map other read errors to `ReadFailed` with a category.
 *   5. UTF-8 decode (substituting U+FFFD for malformed sequences) and
 *      delegate to the parser.
 *
 * Never throws — caller branches on the discriminated union result.
 */
export async function loadArtifactSummary(
  workspaceRoot: string,
  featureName: string,
  artifact: ArtifactKind
): Promise<LoadResult> {
  const featureDir = join(specsRoot(workspaceRoot), featureName);
  if (!existsSync(featureDir)) {
    return { ok: false, error: { kind: 'FeatureNotFound', featureName } };
  }

  const artifactPath = resolveArtifactPath(
    workspaceRoot,
    featureName,
    artifact
  );

  let read: Awaited<ReturnType<typeof readBoundedFile>>;
  try {
    read = await readBoundedFile(artifactPath, ARTIFACT_MAX_BYTES);
  } catch (err) {
    const code = (err as NodeJS.ErrnoException | undefined)?.code;
    const message =
      err instanceof Error ? err.message : String(err ?? 'unknown error');
    if (code === 'ENOENT') {
      return {
        ok: false,
        error: {
          kind: 'ArtifactNotFound',
          featureName,
          artifact,
          path: artifactPath,
        },
      };
    }
    return {
      ok: false,
      error: {
        kind: 'ReadFailed',
        path: artifactPath,
        category: categorizeFsError(code),
        message,
      },
    };
  }

  if ('tooLarge' in read) {
    return {
      ok: false,
      error: {
        kind: 'TooLarge',
        path: artifactPath,
        sizeBytes: read.sizeBytes,
      },
    };
  }

  // Buffer.toString('utf8') substitutes U+FFFD for malformed sequences,
  // satisfying the robustness invariant.
  const source = read.bytes.toString('utf8');
  const summary = parseArtifact(artifact, source);
  return { ok: true, summary };
}
