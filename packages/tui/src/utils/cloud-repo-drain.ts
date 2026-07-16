import type {
  SourceProviderResource,
  SourceProviderResourcePage,
  SourceProviderResourcesRequest,
} from '@kiro/acp-type-covenant';

import { dedupeRepoResources } from './repo-multiselect';

/** One provider's paged resource lister (the `/repo` catalog read). */
export type ListProviderResources = (
  request: SourceProviderResourcesRequest
) => Promise<SourceProviderResourcePage | undefined>;

export interface DrainedRepos {
  /** Deduped resources across every drained provider. */
  resources: SourceProviderResource[];
  /** True when a provider hit the page cap with pages left — the list is a lower bound. */
  partial: boolean;
}

/**
 * Drain every connected provider's repo catalog into one deduped list for the
 * startup checklist count and footer branch. A single page from a single
 * provider undercounts when repos span several providers or more than one page,
 * so page each provider to exhaustion — bounded by maxPages as a safety valve
 * against a pathological catalog (partial=true when that cap cuts a drain short).
 */
export async function drainConnectedProviderRepos(
  list: ListProviderResources,
  providerTypes: string[],
  maxPages = 10
): Promise<DrainedRepos> {
  const all: SourceProviderResource[] = [];
  let partial = false;
  for (const providerType of providerTypes) {
    let cursor: string | undefined;
    let pages = 0;
    do {
      const page = await list({ providerType, cursor });
      if (!page) break;
      all.push(...page.resources);
      cursor = page.nextCursor;
      pages += 1;
      if (cursor && pages >= maxPages) {
        partial = true;
        break;
      }
    } while (cursor);
  }
  return { resources: dedupeRepoResources(all), partial };
}
