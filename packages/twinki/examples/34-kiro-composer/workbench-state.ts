import type { OpenFile } from '../32-acp-showcase/types.js';
import { allWidgets, type LayoutNode, type WidgetNode } from './layout.js';

export interface WorkbenchDocument extends OpenFile {
  sourcePath: string;
  before?: string;
  after?: string;
}

export interface CanvasGroupState {
  tabs: string[];
  activeId: string;
}

export function sessionTabId(id: string): string {
  return `session:${id}`;
}

export function sessionIdFromTab(id: string): string | undefined {
  return id.startsWith('session:') ? id.slice('session:'.length) : undefined;
}

export function fileTabId(path: string): string {
  return `file:${path}`;
}

export function titleFromId(id: string): string {
  return id
    .split(/[-_]/)
    .filter(Boolean)
    .map((part) => part[0]!.toUpperCase() + part.slice(1))
    .join(' ');
}

export function canvasNodes(layout: LayoutNode): WidgetNode[] {
  return allWidgets(layout).filter((node) => node.widget === 'chat' || node.widget === 'editor');
}

export function addMissingCanvasGroups(
  current: Readonly<Record<string, CanvasGroupState>>,
  nodes: readonly WidgetNode[],
  initialDocument: WorkbenchDocument | null,
  selectedSessionId?: string
): Record<string, CanvasGroupState> {
  let next: Record<string, CanvasGroupState> | undefined;
  for (const node of nodes) {
    if (current[node.id]) continue;
    const tabId =
      node.widget === 'chat'
        ? sessionTabId(node.id)
        : initialDocument
          ? fileTabId(initialDocument.path)
          : selectedSessionId
            ? sessionTabId(selectedSessionId)
            : '';
    next ??= { ...current };
    next[node.id] = { tabs: tabId ? [tabId] : [], activeId: tabId };
  }
  return next ?? (current as Record<string, CanvasGroupState>);
}

export function initialCanvasGroups(
  layout: LayoutNode,
  initialDocument: WorkbenchDocument | null,
  selectedSessionId?: string
): Record<string, CanvasGroupState> {
  return addMissingCanvasGroups({}, canvasNodes(layout), initialDocument, selectedSessionId);
}
