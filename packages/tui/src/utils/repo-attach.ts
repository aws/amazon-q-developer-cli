/** Builds the natural-language clone prompt used to attach a repo to a running
 *  cloud session (no dedicated ACP verb exists yet). Pure string helpers. */
import type { SourceProviderList } from '@kiro/acp-type-covenant';

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

/**
 * Instruction covering one `/repo` re-submission: clone what was newly checked
 * AND remove what was unchecked, in a single prompt so one agent turn settles
 * the workspace to exactly the selected set. Either list may be empty; both
 * empty returns `''` (no turn).
 */
export function formatRepoChangeInstruction(
  added: string[],
  removed: string[]
): string {
  const cleanRemoved = removed.map((r) => r.trim()).filter((r) => r.length > 0);
  const parts: string[] = [];
  const clone = formatCloneReposInstruction(added);
  if (clone) parts.push(clone);
  if (cleanRemoved.length > 0) {
    parts.push(
      cleanRemoved.length === 1
        ? `Remove the repository ${cleanRemoved[0]} from the workspace (delete its cloned directory).`
        : `Remove the following repositories from the workspace (delete their cloned directories): ${cleanRemoved.join(', ')}.`
    );
  }
  return parts.join(' ');
}

export interface SourceProviderConnection {
  /** True when at least one provider is connected. */
  connected: boolean;
  /** Display names of every connected provider, in listing order. */
  connectedProviders: string[];
  /** Kiro Web handoff URL to complete a connection, when one is offered. */
  setupUrl?: string;
}

/**
 * Reduce a source-provider listing to "which providers are connected, and if
 * none, where do I send the user to connect". Pure so both the `/repo` picker
 * and the cloud-entry connection check share one interpretation.
 */
export function resolveSourceProviderConnection(
  list: SourceProviderList | undefined
): SourceProviderConnection {
  if (!list) return { connected: false, connectedProviders: [] };
  const connectedProviders = list.providers
    .filter((p) => p.connectionStatus === 'connected')
    .map((p) => p.displayName);
  if (connectedProviders.length > 0) {
    return { connected: true, connectedProviders };
  }
  const handoff = list.providers.find(
    (p) => p.connectionStatus === 'not_connected'
  );
  const setupUrl =
    handoff && handoff.connectionStatus === 'not_connected'
      ? handoff.setupUrl
      : undefined;
  return { connected: false, connectedProviders: [], setupUrl };
}

/**
 * Extract the repo names KAS reported as dropped in `session/new` advisory
 * warnings. Coupled to KAS's warning phrasing (`repository "<name>" ...`);
 * a non-matching warning yields no name and the caller keeps its claim.
 */
export function droppedReposFromWarnings(warnings: string[]): Set<string> {
  return new Set(
    warnings
      .map((w) => /repository "([^"]+)"/.exec(w)?.[1])
      .filter((r): r is string => !!r)
  );
}
