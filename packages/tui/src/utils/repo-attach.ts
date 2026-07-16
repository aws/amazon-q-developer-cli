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

/** The slice of a turn's tool invocations the reconciler inspects. */
export interface RepoTurnTool {
  /** Tool input (command text / JSON args) — where a repo would be named. */
  content: string;
  status?: 'success' | 'error';
}

/**
 * Settle an optimistic picker selection against the turn's per-tool outcomes.
 *
 * A repo's operation is judged failed when some errored tool invocation
 * references it and no successful one does: a failed clone drops the repo
 * from the selection, a failed removal restores it (appended in their prior
 * order). Repos without failure evidence keep their optimistic state — the
 * turn is a natural-language instruction, so an unmentioned repo is not
 * evidence either way, and over-dropping would hide work that did happen.
 * Returns the input `selected` array unchanged when nothing needs to move.
 */
export function reconcileRepoSelection(
  selected: string[],
  added: string[],
  removed: string[],
  turnTools: RepoTurnTool[]
): string[] {
  const failedTools = turnTools.filter((t) => t.status === 'error');
  const succeededTools = turnTools.filter((t) => t.status === 'success');
  if (failedTools.length === 0) return selected;
  const opFailed = (repo: string): boolean =>
    failedTools.some((t) => toolMentionsRepo(t.content, repo)) &&
    !succeededTools.some((t) => toolMentionsRepo(t.content, repo));
  const droppedAdds = new Set(added.filter(opFailed));
  const restoredRemovals = removed.filter(opFailed);
  if (droppedAdds.size === 0 && restoredRemovals.length === 0) return selected;
  return [
    ...selected.filter((repo) => !droppedAdds.has(repo)),
    ...restoredRemovals,
  ];
}

/**
 * Whether a tool invocation references `owner/name`. Clone commands carry the
 * full slug inside the URL; removals typically name only the final path
 * segment, so that segment is also matched on its own (delimited, to keep
 * `app` from matching `my-app`).
 */
function toolMentionsRepo(content: string, repo: string): boolean {
  if (content.includes(repo)) return true;
  const name = repo.split('/').pop() ?? repo;
  const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  return new RegExp(`(^|[^\\w-])${escaped}([^\\w-]|$)`).test(content);
}
