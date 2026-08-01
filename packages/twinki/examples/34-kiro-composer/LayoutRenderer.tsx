import React from 'react';
import { Box, Split, Tabs, Text, type Tab } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';
import { allWidgets, containsWidget, splitSizes, type LayoutNode, type TabsNode } from './layout.js';
import type { WorkbenchWidget } from './widgets.js';

export function LayoutRenderer({
  node,
  width,
  height,
  left,
  top,
  registry,
  activeWidget,
  activeTabs,
  ratios,
  gitDirty,
  theme,
  onActivateTab,
  onResize,
}: {
  node: LayoutNode;
  width: number;
  height: number;
  left: number;
  top: number;
  registry: ReadonlyMap<string, WorkbenchWidget>;
  activeWidget: string;
  activeTabs: Readonly<Record<string, string>>;
  ratios: Readonly<Record<string, number>>;
  gitDirty: boolean;
  theme: ShowcaseTheme;
  onActivateTab: (node: TabsNode, tabId: string) => void;
  onResize: (splitId: string, ratio: number) => void;
}): React.ReactElement {
  if (node.type === 'widget') {
    const widget = registry.get(node.widget);
    if (!widget) {
      return (
        <Box width={width} height={height} padding={1} backgroundColor={theme.panel}>
          <Text color={theme.danger}>Unknown widget: {node.widget}</Text>
        </Box>
      );
    }
    return widget.render({
      id: node.id,
      title: node.title ?? widget.title,
      width,
      height,
      left,
      top,
      active: activeWidget === node.id,
    });
  }

  if (node.type === 'tabs') {
    const activeId = activeTabs[node.id] ?? node.defaultTab;
    const activeTab = node.tabs.find((tab) => tab.id === activeId) ?? node.tabs[0]!;
    const tabs: Tab[] = node.tabs.map((tab) => ({
      id: tab.id,
      title: tab.title,
      dirty: gitDirty && allWidgets(tab.child).some((item) => item.widget === 'git'),
    }));
    return (
      <Box width={width} height={height} flexDirection="column" backgroundColor={theme.panel}>
        <Tabs
          tabs={tabs}
          activeId={activeTab.id}
          onActivate={(id) => onActivateTab(node, id)}
          width={width}
          activeColor={theme.panel}
          activeTextColor={theme.accent}
          inactiveColor={theme.muted}
          borderColor={theme.border}
          stripColor={theme.raised}
        />
        <LayoutRenderer
          node={activeTab.child}
          width={width}
          height={Math.max(1, height - 1)}
          left={left}
          top={top + 1}
          registry={registry}
          activeWidget={activeWidget}
          activeTabs={activeTabs}
          ratios={ratios}
          gitDirty={gitDirty}
          theme={theme}
          onActivateTab={onActivateTab}
          onResize={onResize}
        />
      </Box>
    );
  }

  const ratio = ratios[node.id] ?? node.ratio;
  const sizes = splitSizes(node, ratio, width, height);
  const renderChild = (index: 0 | 1): React.ReactElement => {
    const size = sizes[index];
    return (
      <LayoutRenderer
        node={node.children[index]}
        width={size.width}
        height={size.height}
        left={left + size.x}
        top={top + size.y}
        registry={registry}
        activeWidget={activeWidget}
        activeTabs={activeTabs}
        ratios={ratios}
        gitDirty={gitDirty}
        theme={theme}
        onActivateTab={onActivateTab}
        onResize={onResize}
      />
    );
  };
  return (
    <Split
      direction={node.direction}
      ratio={ratio}
      width={width}
      height={height}
      activePane={containsWidget(node.children[0], activeWidget, activeTabs) ? 'a' : 'b'}
      activeColor={theme.accent}
      inactiveColor={theme.border}
      showPaneBorders={false}
      onResize={(value) => onResize(node.id, value)}
    >
      {renderChild(0)}
      {renderChild(1)}
    </Split>
  );
}
