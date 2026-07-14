/** Builds the natural-language clone prompt used to attach a repo to a running
 *  cloud session (no dedicated ACP verb exists yet). Pure string helpers. */

/**
 * Normalize a raw `/repo` argument to a repo bind value, or `undefined` when
 * empty/whitespace. Accepts the same forms as the create-time bind (e.g.
 * `owner/repo`, `gitlab:group/project`, or a package name) — passed through
 * verbatim (trimmed); the agent resolves it on the sandbox.
 */
export function normalizeRepoArg(raw: string | undefined): string | undefined {
  const trimmed = (raw ?? '').trim();
  return trimmed.length > 0 ? trimmed : undefined;
}

/**
 * Build the natural-language clone instruction sent as an ordinary
 * `session/prompt`. The agent interprets it as a `git clone` of `repo` into
 * the current sandbox workspace.
 */
export function formatCloneRepoInstruction(repo: string): string {
  return `Clone the repository ${repo} into the workspace.`;
}

/**
 * Clone instruction for one or more repos (the `/repo` multi-select result).
 * Blank/whitespace entries are dropped; a single repo uses the single-repo
 * form; multiple repos are listed in one prompt so the agent clones them all.
 * Returns `''` when nothing usable is selected.
 */
export function formatCloneReposInstruction(repos: string[]): string {
  const clean = repos.map((r) => r.trim()).filter((r) => r.length > 0);
  if (clean.length === 0) return '';
  if (clean.length === 1) return formatCloneRepoInstruction(clean[0]!);
  return `Clone the following repositories into the workspace: ${clean.join(', ')}.`;
}
