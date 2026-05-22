/**
 * Spec artifact parser.
 *
 * Parses requirements.md / design.md / tasks.md into a high-signal
 * `ArtifactSummary` for the structured artifact view.
 *
 * Robustness invariants enforced here:
 *   - Same input bytes always produce the same output (pure, deterministic).
 *   - Empty / whitespace-only input produces an empty extracted set.
 *   - Invalid UTF-8 is normalised by Node's Buffer.toString('utf8'), which
 *     substitutes U+FFFD for malformed sequences. The 10 MB cap is enforced
 *     at the loader boundary, not here.
 *
 * The summary is deliberately lossy:
 *   - Acceptance criteria do NOT appear in `RequirementItem` fields apart
 *     from `detailBody`.
 *   - Sub-task text does NOT appear in any HighLevelTask field apart from
 *     `subTasks[]` and `detailBody` — the summary only exposes count / titles.
 */

import { extractRequirements } from './extract-requirements.js';
import { extractDesign } from './extract-design.js';
import { extractTasks } from './extract-tasks.js';
import type { ArtifactKind, ArtifactSummary } from './types.js';

export { extractRequirements, extractDesign, extractTasks };

export type {
  ArtifactKind,
  ArtifactSummary,
  RequirementItem,
  DesignSection,
  HighLevelTask,
  SubTask,
} from './types.js';

/**
 * Convert raw bytes (Buffer) to a UTF-8 string, substituting U+FFFD for any
 * malformed sequences. This is what Node/Bun's `Buffer.toString('utf8')`
 * does by default, but exposing it as a helper makes the contract explicit.
 */
export function bytesToUtf8(bytes: Buffer): string {
  return bytes.toString('utf8');
}

/**
 * Parse an artifact's source text into the corresponding `ArtifactSummary`.
 *
 * Pure: same input produces the same output. Never throws.
 */
export function parseArtifact(
  kind: ArtifactKind,
  source: string
): ArtifactSummary {
  switch (kind) {
    case 'requirements':
      return { kind: 'requirements', items: extractRequirements(source) };
    case 'design': {
      const { overview, overviewTruncated, sections } = extractDesign(source);
      return { kind: 'design', overview, overviewTruncated, sections };
    }
    case 'tasks':
      return { kind: 'tasks', items: extractTasks(source) };
  }
}
