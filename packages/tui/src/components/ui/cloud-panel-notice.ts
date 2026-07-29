import type { CloudSnapshotReadiness } from '../../stores/app-store.js';
import { visibleWidth } from '../../utils/text-width.js';

/** Panels that show a provenance notice while a cloud session is active. */
export type CloudNoticePanel = 'mcp' | 'tools' | 'hooks';

/** The cloud-session notice for a backend panel, or undefined. */
export function cloudPanelNotice(
  panel: CloudNoticePanel,
  cloudSessionActive: boolean,
  readiness?: CloudSnapshotReadiness
): string | undefined {
  if (!cloudSessionActive) return undefined;
  if (panel === 'hooks') return 'Hooks fetched from the cloud sandbox';
  if (readiness === 'awaiting-sandbox') {
    return 'Cloud sandbox configuration not yet received — it will appear when the sandbox reports it.';
  }
  return undefined;
}

/** Empty state when the sandbox authoritatively reported no entries. */
export function cloudPanelEmptyMessage(panel: 'mcp' | 'tools'): string {
  return panel === 'mcp'
    ? 'The cloud sandbox has no MCP servers configured'
    : 'The cloud sandbox has no tools available';
}

/** Lines the notice occupies (soft-wrap + margin); panels shrink their row window by this. */
export function cloudNoticeLineCount(
  notice: string | undefined,
  contentWidth: number,
  hasRowsBelow: boolean
): number {
  if (!notice) return 0;
  const width = Math.max(contentWidth, 1);
  return Math.ceil(visibleWidth(notice) / width) + (hasRowsBelow ? 1 : 0);
}
