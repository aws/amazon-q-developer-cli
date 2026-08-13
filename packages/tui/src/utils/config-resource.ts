/**
 * Reader for the KAS `ConfigResource` descriptor (kiro-agent PR #2141,
 * design: Pippin "KAS Cloud Config ACP API").
 *
 * KAS attaches `{ resourceType, source }` at `_meta.kiro.resource` on the
 * per-item objects of the notifications it already emits (MCP status,
 * steering documents, progressive-context items, hooks, powers, slash
 * commands, session modes). `source` is a discriminated union:
 *
 *   bundled | client | user | workspace | cloud
 *   | { origin: 'power', power: { name, source: <direct> } }
 *
 * The CLI's config UX only needs the local-vs-cloud fact, so this module
 * collapses the union: `cloud` → 'cloud', everything else → 'local', with a
 * power-delivered resource taking its POWER's direct source (a
 * cloud-distributed power's servers are cloud config). Phase 1 descriptors
 * carry no identifier (KRN deferred); per the covenant, clients key on the
 * item's native name fields and must not condition behavior on descriptor
 * ABSENCE — callers fall back to session placement when this returns
 * undefined.
 */

import type { ConfigSource } from '../components/ui/config-panel-model.js';

/**
 * Extract the collapsed local/cloud source from a notification item's
 * `_meta.kiro.resource`, or undefined when no descriptor is present or it
 * doesn't parse (older KAS, malformed payload — never an error).
 */
export function configResourceSource(item: unknown): ConfigSource | undefined {
  if (typeof item !== 'object' || item === null) return undefined;
  const meta = (item as { _meta?: unknown })._meta;
  if (typeof meta !== 'object' || meta === null) return undefined;
  const kiro = (meta as { kiro?: unknown }).kiro;
  if (typeof kiro !== 'object' || kiro === null) return undefined;
  const resource = (kiro as { resource?: unknown }).resource;
  if (typeof resource !== 'object' || resource === null) return undefined;
  return collapseSource((resource as { source?: unknown }).source);
}

/**
 * Collapse a `ConfigResourceSource` union value to local/cloud. Unrecognized
 * origins (future covenant arms) collapse to 'local' — the conservative
 * answer for dark-shipped cloud labeling.
 */
function collapseSource(source: unknown): ConfigSource | undefined {
  if (typeof source !== 'object' || source === null) return undefined;
  const origin = (source as { origin?: unknown }).origin;
  if (typeof origin !== 'string') return undefined;
  if (origin === 'cloud') return 'cloud';
  if (origin === 'power') {
    // A power-delivered resource carries the power's own DirectSource
    // nested; the resource is cloud config iff the power came from the
    // cloud replica. One level deep by covenant construction.
    const inner = (source as { power?: { source?: unknown } }).power?.source;
    const innerOrigin =
      typeof inner === 'object' && inner !== null
        ? (inner as { origin?: unknown }).origin
        : undefined;
    return innerOrigin === 'cloud' ? 'cloud' : 'local';
  }
  // bundled | client | user | workspace | future arms
  return 'local';
}
