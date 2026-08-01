export interface WidgetNode {
  type: 'widget';
  id: string;
  widget: string;
  title?: string;
}

export interface SplitNode {
  type: 'split';
  id: string;
  direction: 'row' | 'column';
  ratio: number;
  children: [LayoutNode, LayoutNode];
}

export interface TabsNode {
  type: 'tabs';
  id: string;
  defaultTab: string;
  tabs: Array<{ id: string; title: string; child: LayoutNode }>;
}

export type LayoutNode = WidgetNode | SplitNode | TabsNode;

export interface WorkbenchLayout {
  version: 1;
  title: string;
  layout: LayoutNode;
}

export interface LayoutChoice {
  path: string;
  source: string;
  spec: WorkbenchLayout;
}

export interface WidgetBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

type JsonObject = Record<string, unknown>;

function fail(path: string, message: string): never {
  throw new Error(`${path}: ${message}`);
}

function objectAt(value: unknown, path: string): JsonObject {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    fail(path, 'expected an object');
  }
  return value as JsonObject;
}

function stringAt(value: unknown, path: string): string {
  if (typeof value !== 'string' || !value) {
    fail(path, 'expected a non-empty string');
  }
  return value;
}

function parseNode(
  value: unknown,
  nodeIds: Set<string>,
  registeredWidgets: ReadonlySet<string> | undefined,
  path: string,
  depth: number
): LayoutNode {
  if (depth > 6) fail(path, 'layout nesting is too deep');
  const node = objectAt(value, path);
  const type = stringAt(node.type, `${path}.type`);
  if (type === 'widget') {
    const widget = stringAt(node.widget, `${path}.widget`);
    if (registeredWidgets && !registeredWidgets.has(widget)) {
      fail(`${path}.widget`, `unknown widget "${widget}"`);
    }
    const id = node.id === undefined ? widget : stringAt(node.id, `${path}.id`);
    if (nodeIds.has(id)) fail(`${path}.id`, `duplicate node id "${id}"`);
    nodeIds.add(id);
    const title = node.title === undefined ? undefined : stringAt(node.title, `${path}.title`);
    return { type, id, widget, title };
  }
  const id = stringAt(node.id, `${path}.id`);
  if (nodeIds.has(id)) fail(`${path}.id`, `duplicate node id "${id}"`);
  nodeIds.add(id);
  if (type === 'split') {
    if (node.direction !== 'row' && node.direction !== 'column') {
      fail(`${path}.direction`, 'expected "row" or "column"');
    }
    if (typeof node.ratio !== 'number' || node.ratio < 0.15 || node.ratio > 0.85) {
      fail(`${path}.ratio`, 'expected a number from 0.15 through 0.85');
    }
    if (!Array.isArray(node.children) || node.children.length !== 2) {
      fail(`${path}.children`, 'expected exactly two children');
    }
    return {
      type,
      id,
      direction: node.direction,
      ratio: node.ratio,
      children: [
        parseNode(node.children[0], nodeIds, registeredWidgets, `${path}.children[0]`, depth + 1),
        parseNode(node.children[1], nodeIds, registeredWidgets, `${path}.children[1]`, depth + 1),
      ],
    };
  }
  if (type !== 'tabs') {
    fail(`${path}.type`, 'expected "widget", "split", or "tabs"');
  }
  if (!Array.isArray(node.tabs) || node.tabs.length === 0) {
    fail(`${path}.tabs`, 'expected at least one tab');
  }
  const tabs = node.tabs.map((value, index) => {
    const tab = objectAt(value, `${path}.tabs[${index}]`);
    return {
      id: stringAt(tab.id, `${path}.tabs[${index}].id`),
      title: stringAt(tab.title, `${path}.tabs[${index}].title`),
      child: parseNode(tab.child, nodeIds, registeredWidgets, `${path}.tabs[${index}].child`, depth + 1),
    };
  });
  if (new Set(tabs.map((tab) => tab.id)).size !== tabs.length) {
    fail(`${path}.tabs`, 'tab ids must be unique');
  }
  const defaultTab = stringAt(node.defaultTab, `${path}.defaultTab`);
  if (!tabs.some((tab) => tab.id === defaultTab)) {
    fail(`${path}.defaultTab`, 'must match a tab id');
  }
  return { type, id, defaultTab, tabs };
}

export function parseWorkbenchLayout(value: unknown, registeredWidgets?: Iterable<string>): WorkbenchLayout {
  const root = objectAt(value, '$');
  if (root.version !== 1) fail('$.version', 'expected 1');
  return {
    version: 1,
    title: stringAt(root.title, '$.title'),
    layout: parseNode(
      root.layout,
      new Set(),
      registeredWidgets ? new Set(registeredWidgets) : undefined,
      '$.layout',
      0
    ),
  };
}

export function splitSizes(
  node: SplitNode,
  ratio: number,
  width: number,
  height: number
): [WidgetBounds, WidgetBounds] {
  const available = Math.max(2, (node.direction === 'row' ? width : height) - 1);
  const first = Math.max(1, Math.round(available * ratio));
  const second = Math.max(1, available - first);
  return node.direction === 'row'
    ? [
        { x: 0, y: 0, width: first, height },
        { x: first + 1, y: 0, width: second, height },
      ]
    : [
        { x: 0, y: 0, width, height: first },
        { x: 0, y: first + 1, width, height: second },
      ];
}

function activeTab(node: TabsNode, tabs: Readonly<Record<string, string>>) {
  const id = tabs[node.id] ?? node.defaultTab;
  return node.tabs.find((tab) => tab.id === id) ?? node.tabs[0]!;
}

export function widgetBounds(
  node: LayoutNode,
  width: number,
  height: number,
  ratios: Readonly<Record<string, number>>,
  tabs: Readonly<Record<string, string>>,
  x = 0,
  y = 0,
  result: Record<string, WidgetBounds> = {}
): Record<string, WidgetBounds> {
  if (node.type === 'widget') {
    result[node.id] = { x, y, width, height };
  } else if (node.type === 'tabs') {
    widgetBounds(activeTab(node, tabs).child, width, Math.max(1, height - 1), ratios, tabs, x, y + 1, result);
  } else {
    const sizes = splitSizes(node, ratios[node.id] ?? node.ratio, width, height);
    node.children.forEach((child, index) => {
      const size = sizes[index]!;
      widgetBounds(child, size.width, size.height, ratios, tabs, x + size.x, y + size.y, result);
    });
  }
  return result;
}

export function containsWidget(node: LayoutNode, widget: string, tabs: Readonly<Record<string, string>>): boolean {
  if (node.type === 'widget') return node.id === widget;
  if (node.type === 'tabs') {
    return containsWidget(activeTab(node, tabs).child, widget, tabs);
  }
  return node.children.some((child) => containsWidget(child, widget, tabs));
}

export function allWidgets(node: LayoutNode, result = new Map<string, WidgetNode>()): WidgetNode[] {
  if (node.type === 'widget') result.set(node.id, node);
  else if (node.type === 'tabs') {
    node.tabs.forEach((tab) => allWidgets(tab.child, result));
  } else node.children.forEach((child) => allWidgets(child, result));
  return [...result.values()];
}

export function firstWidget(node: LayoutNode, tabs: Readonly<Record<string, string>>): string {
  if (node.type === 'widget') return node.id;
  if (node.type === 'tabs') return firstWidget(activeTab(node, tabs).child, tabs);
  return firstWidget(node.children[0], tabs);
}

export function tabsForWidget(
  node: LayoutNode,
  widget: string,
  current: Readonly<Record<string, string>>
): Record<string, string> {
  const next = { ...current };
  const visit = (candidate: LayoutNode): boolean => {
    if (candidate.type === 'widget') return candidate.id === widget || candidate.widget === widget;
    if (candidate.type === 'tabs') {
      const tab = candidate.tabs.find((item) =>
        allWidgets(item.child).some((node) => node.id === widget || node.widget === widget)
      );
      if (!tab) return false;
      next[candidate.id] = tab.id;
      return visit(tab.child);
    }
    return candidate.children.some(visit);
  };
  visit(node);
  return next;
}
