import type { ToolInfo } from '../stores/app-store.js';

/**
 * Local mirror of KAS's `SessionToolTag` (from `@kiro/acp-type-covenant`'s
 * `capabilities/tools/list`). Defined locally because the installed covenant
 * version does not yet export it, and `onExtNotification` delivers the payload
 * as `Record<string, unknown>` regardless.
 *
 * One entry per tag available in the session:
 * - `builtin`: a user-facing category tag (e.g. `read`, `write`); individual
 *   tool identifiers are an implementation detail and are not exposed.
 * - `mcp`: a per-tool `@server/tool` tag — exactly the value a customer puts in
 *   an agent profile's `tools: []` allowlist.
 */
export interface SessionToolTag {
  source: 'builtin' | 'mcp';
  tag: string;
  description: string;
}

/** Payload of the `_kiro/tools/didChange` agent → client notification. */
export interface ToolsDidChangeNotification {
  sessionId: string;
  tags: SessionToolTag[];
}

/**
 * Maps a KAS `SessionToolTag` onto the TUI's `ToolInfo` so the shared
 * `ToolsPanel` can render it. KAS sends no permission status, so `status` is
 * left undefined (the panel hides the Status column when absent).
 */
export function mapSessionToolTagToToolInfo(tag: SessionToolTag): ToolInfo {
  return {
    name: tag.tag,
    source: tag.source,
    description: tag.description,
  };
}

/**
 * Parses a raw `_kiro/tools/didChange` notification payload defensively.
 * Returns the mapped `ToolInfo[]`, skipping malformed entries. Non-array or
 * missing `tags` yields an empty list.
 */
export function parseToolsDidChange(
  params: Record<string, unknown>
): ToolInfo[] {
  const rawTags = Array.isArray(params.tags) ? params.tags : [];
  const result: ToolInfo[] = [];
  for (const entry of rawTags) {
    if (entry === null || typeof entry !== 'object') continue;
    const t = entry as Record<string, unknown>;
    const tag = typeof t.tag === 'string' ? t.tag : undefined;
    const source =
      t.source === 'builtin' || t.source === 'mcp' ? t.source : undefined;
    if (tag === undefined || source === undefined) continue;
    const description = typeof t.description === 'string' ? t.description : tag;
    result.push(mapSessionToolTagToToolInfo({ source, tag, description }));
  }
  return result;
}
