import { UNICODE_GLYPHS, type Glyphs } from './glyphs';

/**
 * The cloud sandbox workspace path a bound repo is cloned into. A cloud session
 * runs inside the sandbox, not the user's local checkout, so the footer must
 * name the sandbox's working directory — `~/kiro/<repo-name>` — rather than the
 * `owner/name` binding or the local cwd. Only the repo's last path segment is
 * used (the binding's `owner/` prefix isn't part of the on-sandbox path), e.g.
 * `kiro-team/banana-service` → `~/kiro/banana-service`. Returns `null` for an
 * absent/blank repo (a New empty sandbox has no workspace path).
 */
export function cloudWorkspacePath(repo?: string | null): string | null {
  const trimmed = repo?.trim();
  if (!trimmed) return null;
  const name = trimmed.split('/').pop()!.trim();
  if (!name) return null;
  return `~/kiro/${name}`;
}

/**
 * Persistent cloud-session footer label (not the activity status). Renders
 * `<icon> Cloud (Preview) · ~/kiro/<repo-name> · <branch> (+N other[s])`,
 * dropping any segment that is absent. Cloud sessions are a preview feature,
 * so the label carries the "(Preview)" qualifier (KIRONEXT-1).
 *  - A New empty sandbox (no repo) shows just `<icon> Cloud (Preview)`.
 *  - One bound repo shows `~/kiro/<repo> · <branch>`.
 *  - More than one bound repo appends `(+N other)`/`(+N others)` after the
 *    first repo segment (including its branch when present), where N =
 *    `others` (the count of additional repos beyond the one shown; singular
 *    "other" when N is 1) —
 *    e.g. `~/kiro/repo · main (+1 other)`, `~/kiro/banana-service · main (+3 others)`.
 * The repo segment is the sandbox workspace path (see {@link cloudWorkspacePath}),
 * so the footer reflects where the session runs — the cloud sandbox — not the
 * local checkout. `icon` and the segment separators degrade in ASCII mode via
 * `glyphs`; omit `icon` for no badge.
 */
export function formatCloudFooter(
  repo?: string | null,
  branch?: string | null,
  icon?: string,
  others?: number | null,
  glyphs: Glyphs = UNICODE_GLYPHS
): string {
  const dot = glyphs.smallDot;
  const prefix = icon ? `${icon} ` : '';
  const parts = ['Cloud (Preview)'];
  const workspacePath = cloudWorkspacePath(repo);
  if (workspacePath) {
    let repoSegment = workspacePath;
    const trimmedBranch = branch?.trim();
    if (trimmedBranch) repoSegment = `${repoSegment} ${dot} ${trimmedBranch}`;
    // The `(+N other[s])` suffix rides the repo segment (after the branch) so
    // it reads as "this repo + N more", not as its own separator-delimited
    // field.
    if (others && others > 0)
      repoSegment = `${repoSegment} (+${others} ${others === 1 ? 'other' : 'others'})`;
    parts.push(repoSegment);
  }
  return `${prefix}${parts.join(` ${dot} `)}`;
}
