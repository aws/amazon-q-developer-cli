import type React from 'react';

export interface WidgetFrame {
  id: string;
  title: string;
  width: number;
  height: number;
  left: number;
  top: number;
  active: boolean;
}

export const BUILTIN_WIDGET_IDS = ['chat', 'files', 'git', 'session', 'settings', 'editor'] as const;
export type BuiltinWidget = (typeof BUILTIN_WIDGET_IDS)[number];

export const BUILTIN_WIDGET_CAPABILITIES: Readonly<Record<BuiltinWidget, string>> = {
  chat: 'Canvas slot initialized with an ACP session; it can also host files, diffs, and other sessions as tabs.',
  editor: 'Canvas slot initialized with a workspace file; it can also host diffs and ACP sessions as tabs.',
  files: 'Hierarchical workspace explorer with file tabs, split-left/right actions, and prompt context actions.',
  git: 'Changed-file navigator that opens responsive side-by-side or unified diff tabs.',
  session: 'Manager for creating, renaming, focusing, and closing independent ACP sessions.',
  settings: 'Theme selector and Ask or YOLO permission mode.',
};

export interface WorkbenchWidget {
  id: string;
  title: string;
  render: (frame: WidgetFrame) => React.ReactElement;
}

export function defineWidget(widget: WorkbenchWidget): WorkbenchWidget {
  return widget;
}

export function createWidgetRegistry(widgets: WorkbenchWidget[]): ReadonlyMap<string, WorkbenchWidget> {
  const registry = new Map<string, WorkbenchWidget>();
  for (const widget of widgets) {
    if (registry.has(widget.id)) {
      throw new Error(`Duplicate workbench widget "${widget.id}"`);
    }
    registry.set(widget.id, widget);
  }
  return registry;
}
