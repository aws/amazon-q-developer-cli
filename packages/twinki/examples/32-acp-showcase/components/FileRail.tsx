import React, { useEffect, useMemo, useState } from "react";
import { Box, Scrollbar, Text, useMouse, type ComponentMouseEvent } from "twinki";
import type { FileEntry } from "../types.js";
import type { ShowcaseTheme } from "../themes.js";

interface TreeNode {
  id: string;
  label: string;
  children: TreeNode[];
  file?: FileEntry;
}
interface TreeRow {
  node: TreeNode;
  depth: number;
  expanded: boolean;
}

function buildTree(files: FileEntry[]): TreeNode[] {
  const root: TreeNode = { id: "", label: "", children: [] };
  for (const file of files) {
    const parts = file.relativePath.split(/[\\/]/).filter(Boolean);
    let parent = root;
    parts.forEach((label, index) => {
      const id = parts.slice(0, index + 1).join("/");
      let node = parent.children.find((child) => child.id === id);
      if (!node) {
        node = { id, label, children: [] };
        parent.children.push(node);
      }
      if (index === parts.length - 1) node.file = file;
      parent = node;
    });
  }
  const sort = (nodes: TreeNode[]): void => {
    nodes.sort(
      (left, right) =>
        Number(Boolean(left.file)) - Number(Boolean(right.file)) || left.label.localeCompare(right.label),
    );
    nodes.forEach((node) => sort(node.children));
  };
  sort(root.children);
  return root.children;
}

function flatten(nodes: TreeNode[], expanded: ReadonlySet<string>, depth = 0, rows: TreeRow[] = []): TreeRow[] {
  for (const node of nodes) {
    const open = !node.file && expanded.has(node.id);
    rows.push({ node, depth, expanded: open });
    if (open) flatten(node.children, expanded, depth + 1, rows);
  }
  return rows;
}

export interface FileRailProps {
  files: FileEntry[];
  selectedPath?: string;
  width: number;
  height: number;
  theme: ShowcaseTheme;
  onOpen: (file: FileEntry) => void;
  onContext: (file: FileEntry, event: ComponentMouseEvent) => void;
}

export function FileRail({
  files,
  selectedPath,
  width,
  height,
  theme,
  onOpen,
  onContext,
}: FileRailProps): React.ReactElement {
  const tree = useMemo(() => buildTree(files), [files]);
  const [expanded, setExpanded] = useState(
    () =>
      new Set(
        files
          .map((file) => file.relativePath.split(/[\\/]/))
          .filter((parts) => parts.length > 1)
          .map((parts) => parts[0]!),
      ),
  );
  const [hovered, setHovered] = useState<string | null>(null);
  const [start, setStart] = useState(0);
  const rows = useMemo(() => flatten(tree, expanded), [expanded, tree]);
  const viewport = Math.max(1, height - 1);
  const maxStart = Math.max(0, rows.length - viewport);
  const visible = rows.slice(start, start + viewport);

  useEffect(() => setStart((value) => Math.min(value, maxStart)), [maxStart]);
  useMouse((event) => {
    if (event.x >= width) return;
    if (event.type !== "scrollup" && event.type !== "scrolldown") return;
    const delta = event.type === "scrollup" ? -3 : 3;
    setStart((value) => Math.max(0, Math.min(maxStart, value + delta)));
  });

  const activate = (node: TreeNode): void => {
    if (node.file) {
      onOpen(node.file);
      return;
    }
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(node.id)) next.delete(node.id);
      else next.add(node.id);
      return next;
    });
  };

  return (
    <Box flexDirection="column" width={width} height={height} backgroundColor={theme.panel}>
      <Box height={1} paddingX={1} backgroundColor={theme.raised} selectionScope>
        <Text color={theme.muted} bold>
          {`FILES ${files.length}`}
        </Text>
      </Box>
      <Box flexDirection="row" height={viewport}>
        <Box flexDirection="column" width={Math.max(1, width - 1)} selectionScope>
          {visible.map(({ node, depth, expanded: open }) => {
            const selected = node.file?.path === selectedPath;
            const hot = selected || node.id === hovered;
            const marker = node.file ? " " : open ? "-" : "+";
            return (
              <Box
                key={node.id}
                height={1}
                paddingLeft={1}
                backgroundColor={hot ? theme.raised : theme.panel}
                onClick={() => activate(node)}
                onMouseDown={(event) => {
                  if (node.file && event.button === "right") {
                    onContext(node.file, event);
                  }
                }}
                onMouseEnter={() => setHovered(node.id)}
                onMouseLeave={() => setHovered(null)}
              >
                <Text
                  color={selected ? theme.accent : node.file ? theme.fg : theme.warning}
                  bold={selected || !node.file}
                  wrap="truncate"
                >
                  {`${"  ".repeat(depth)}${marker} ${node.label}`}
                </Text>
              </Box>
            );
          })}
          {rows.length === 0 && (
            <Box paddingX={1}>
              <Text color={theme.muted}>No readable files</Text>
            </Box>
          )}
        </Box>
        <Scrollbar
          scrollTop={start}
          totalLines={rows.length}
          viewportHeight={viewport}
          color={theme.border}
          thumbColor={theme.accent}
          onScrollTo={setStart}
        />
      </Box>
    </Box>
  );
}
