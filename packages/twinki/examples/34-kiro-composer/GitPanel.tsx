import React, { useEffect, useMemo, useState } from 'react';
import { Box, Scrollbar, Text, useMouse, type ComponentMouseEvent } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';
import type { GitFile, GitSnapshot } from './git.js';

function fileColor(file: GitFile, theme: ShowcaseTheme): string {
  if (file.code.includes('U')) return theme.danger;
  if (file.untracked) return theme.warning;
  if (file.staged && !file.unstaged) return theme.success;
  return theme.accent;
}

export function GitPanel({
  snapshot,
  selectedPath,
  width,
  height,
  left,
  top,
  theme,
  onOpen,
  onContext,
  onRefresh,
}: {
  snapshot: GitSnapshot;
  selectedPath?: string;
  width: number;
  height: number;
  left: number;
  top: number;
  theme: ShowcaseTheme;
  onOpen: (file: GitFile) => void;
  onContext: (file: GitFile, event: ComponentMouseEvent) => void;
  onRefresh: () => void;
}): React.ReactElement {
  const [start, setStart] = useState(0);
  const viewport = Math.max(1, height - 2);
  const maxStart = Math.max(0, snapshot.files.length - viewport);
  const visible = snapshot.files.slice(start, start + viewport);
  const summary = useMemo(() => {
    const staged = snapshot.files.filter((file) => file.staged).length;
    const changed = snapshot.files.filter((file) => file.unstaged).length;
    const untracked = snapshot.files.filter((file) => file.untracked).length;
    return `${staged} staged  ${changed} changed  ${untracked} new`;
  }, [snapshot.files]);

  useEffect(() => setStart((value) => Math.min(value, maxStart)), [maxStart]);
  useMouse((event) => {
    if (event.x < left || event.x >= left + width || event.y < top || event.y >= top + height) {
      return;
    }
    if (event.type !== 'scrollup' && event.type !== 'scrolldown') return;
    const delta = event.type === 'scrollup' ? -3 : 3;
    setStart((value) => Math.max(0, Math.min(maxStart, value + delta)));
  });

  return (
    <Box flexDirection="column" width={width} height={height} backgroundColor={theme.panel}>
      <Box height={1} paddingX={1} justifyContent="space-between" backgroundColor={theme.raised}>
        <Text color={theme.fg} bold wrap="truncate-middle">
          {snapshot.error ? 'SOURCE CONTROL' : snapshot.branch || 'HEAD'}
        </Text>
        <Text color={snapshot.loading ? theme.muted : theme.accent} bold onClick={onRefresh}>
          {snapshot.loading ? ' ... ' : ' ↻ '}
        </Text>
      </Box>
      <Box height={1} paddingX={1}>
        <Text color={theme.muted} wrap="truncate">
          {snapshot.error ? snapshot.error : summary}
        </Text>
      </Box>
      <Box flexDirection="row" height={viewport}>
        <Box flexDirection="column" width={Math.max(1, width - 1)}>
          {visible.map((file) => {
            const selected = file.path === selectedPath;
            return (
              <Box
                key={`${file.code}:${file.path}`}
                height={1}
                paddingX={1}
                backgroundColor={selected ? theme.raised : theme.panel}
                onClick={() => onOpen(file)}
                onMouseDown={(event) => {
                  if (event.button === 'right') onContext(file, event);
                }}
              >
                <Box width={4}>
                  <Text color={fileColor(file, theme)} bold>
                    {file.code}
                  </Text>
                </Box>
                <Text color={selected ? theme.fg : theme.muted} bold={selected} wrap="truncate-middle">
                  {file.path}
                </Text>
              </Box>
            );
          })}
          {!snapshot.error && !snapshot.loading && snapshot.files.length === 0 && (
            <Box paddingX={1}>
              <Text color={theme.success}>Working tree clean</Text>
            </Box>
          )}
        </Box>
        <Scrollbar
          scrollTop={start}
          totalLines={snapshot.files.length}
          viewportHeight={viewport}
          color={theme.border}
          thumbColor={theme.accent}
          onScrollTo={setStart}
        />
      </Box>
    </Box>
  );
}
