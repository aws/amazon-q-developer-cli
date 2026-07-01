/**
 * LiteTaskTray — todo/task viewer for the lite UI. Reads `tasks` /
 * `activityTrayExpanded` from app-store (populated by the same path the modern
 * TUI uses). Purely presentational: the Ctrl+X toggle keypress lives in
 * LiteLayout so it can be gated on app-level state.
 */
import React from 'react';
import { Box, Text } from '../../../renderer.js';
import { useAppStore } from '../../../stores/app-store.js';
import { wrapAtWords } from '../../../lite/render.js';
import { getVerboseDisplay } from '../../../lite/verbose.js';
import { visibleWidth } from '../../../utils/text-width.js';
import { useGlyphs, useAllowIcons } from '../../../hooks/useGlyphs.js';
import chalk from 'chalk';

const MAX_VISIBLE_LINES = 6;

export const LiteTaskTray: React.FC = () => {
  const tasks = useAppStore((s) => s.tasks);
  const expanded = useAppStore((s) => s.activityTrayExpanded);
  // glyphs swap Unicode⇄ASCII; allowIcons replaces the status icon with a
  // space (keeps column alignment) when icons are disabled.
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();

  if (tasks.length === 0) return null;
  // /verbose · Task list off suppresses the tray independently of tool-call
  // filters. Tasks stay tracked in the store; flipping back on re-renders.
  if (!getVerboseDisplay().showTasks) return null;

  const done = tasks.filter((t) => t.status === 'completed').length;
  const remaining = tasks.length - done;

  // Glyph-sourced so ASCII mode gets `.` instead of `·`.
  const sep = chalk.dim(` ${glyphs.smallDot} `);

  if (!expanded) {
    const parts: string[] = [chalk.bold('tasks')];
    if (done > 0) parts.push(chalk.green(`${done} done`));
    if (remaining > 0) parts.push(`${remaining} remaining`);
    return (
      <Text>
        {'  '}
        {parts.join(sep)}
        {chalk.dim(`${' '}${glyphs.smallDot} ctrl+x to expand`)}
      </Text>
    );
  }

  // Auto-follow: shift scrollOffset so the next pending row sits just inside
  // the top of the visible window when the list overflows the viewport.
  const nextIndex = tasks.findIndex((t) => t.status !== 'completed');
  let scrollOffset = 0;
  if (tasks.length > MAX_VISIBLE_LINES) {
    const target = nextIndex === -1 ? tasks.length - 1 : nextIndex;
    const maxScroll = tasks.length - MAX_VISIBLE_LINES;
    scrollOffset = Math.min(maxScroll, Math.max(0, target - 1));
  }
  const visible = tasks.slice(scrollOffset, scrollOffset + MAX_VISIBLE_LINES);
  const hiddenAbove = scrollOffset;
  const hiddenBelow = tasks.length - scrollOffset - visible.length;

  const headerParts: string[] = [
    chalk.bold('tasks') + chalk.dim(` (${tasks.length})`),
  ];
  if (done > 0) headerParts.push(chalk.green(`${done} done`));
  if (remaining > 0) headerParts.push(`${remaining} remaining`);
  headerParts.push(chalk.dim('ctrl+x to collapse'));
  const header = `  ${headerParts.join(sep)}`;

  return (
    <Box flexDirection="column">
      <Text>{header}</Text>
      {hiddenAbove > 0 && (
        <Text>{chalk.dim(`  ${glyphs.smallDot} ${hiddenAbove} above`)}</Text>
      )}
      {visible.map((task, i) => {
        const globalIndex = scrollOffset + i;
        const isLast = globalIndex === tasks.length - 1;
        const isNext = globalIndex === nextIndex;

        // treeCorner only on the actual final task; treeBranch otherwise
        // (incl. the last visible row when the list continues below).
        const connector =
          isLast && hiddenBelow === 0
            ? glyphs.treeCorner
            : i === visible.length - 1 && hiddenBelow > 0
              ? glyphs.treeBranch
              : isLast
                ? glyphs.treeCorner
                : glyphs.treeBranch;

        let icon: string;
        if (!allowIcons) {
          icon = ' ';
        } else if (task.status === 'completed') {
          icon = chalk.green(glyphs.checkmark);
        } else if (isNext) {
          icon = chalk.cyan(glyphs.arrowRight);
        } else {
          icon = chalk.dim(glyphs.dotEmpty);
        }

        // Applied per-wrapped-line so each row carries its own SGR open/close
        // — no ANSI sequence ever spans a hard line break.
        const styleLine = (s: string): string => {
          if (task.status === 'completed') return chalk.dim.strikethrough(s);
          if (isNext) return chalk.bold(s);
          return s;
        };

        // Visible-col width of the first-line prefix: '  '(2) + connector(3)
        // + ' '(1) + icon(1) + ' '(1) + `${id}.`(idWidth+1) + ' '(1). Continuation
        // lines indent by exactly this so wrapped subject text aligns with the
        // first-line subject instead of col 0.
        const prefixWidth = 9 + visibleWidth(task.id);
        const cols = process.stdout.columns ?? 80;
        // Floor at 20 cols so a narrow terminal doesn't degenerate into
        // one-char-per-line wrap (terminal soft-wrap takes back over there).
        const availWidth = Math.max(20, cols - prefixWidth);
        const wrapped = wrapAtWords(task.subject, availWidth, availWidth);
        const indent = ' '.repeat(prefixWidth);

        // Rendered as a single <Text wrap="overflow"> with embedded '\n' so
        // twinki passes the newlines through verbatim (no re-wrap at col 0).
        const head = `  ${chalk.dim(connector)} ${icon} ${chalk.dim(`${task.id}.`)} ${styleLine(wrapped[0] ?? '')}`;
        const tail = wrapped
          .slice(1)
          .map((line) => `${indent}${styleLine(line)}`);
        const body = tail.length > 0 ? `${head}\n${tail.join('\n')}` : head;

        return (
          <Text key={task.id} wrap="overflow">
            {body}
          </Text>
        );
      })}
      {hiddenBelow > 0 && (
        <Text>{chalk.dim(`  ${glyphs.smallDot} ${hiddenBelow} below`)}</Text>
      )}
    </Box>
  );
};
