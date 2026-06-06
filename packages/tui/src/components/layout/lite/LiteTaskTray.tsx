/**
 * LiteTaskTray — todo/task viewer for the lite UI.
 *
 * Mirrors the modern TUI's <ActivityTray /> for the tasks half only. Queued
 * messages are surfaced separately above the input in lite (see LiteLayout's
 * `queuedMessages.length > 0` block), so this tray sticks to tasks.
 *
 * State source: `tasks` and `activityTrayExpanded` in the global app-store.
 * Tasks are populated by the agent's `todo_list` / `task` tool calls via
 * `extractTaskState` in app-store.ts — the same path the modern TUI uses,
 * so the data is already there regardless of UI mode.
 *
 * Two states:
 *   - Collapsed (default): one-line summary
 *       "  tasks · X done · Y remaining · ctrl+x to expand"
 *   - Expanded: header + tree-style list with status icons, capped at
 *     MAX_VISIBLE_LINES rows. Auto-follow scroll keeps the next pending task
 *     in view, with "+N above / +N below" markers when the list overflows.
 *
 * Toggle: Ctrl+X. The actual keypress lives in LiteLayout so it can be
 * gated on app-level state (no editing-queue, no approval, no backend
 * panel). This component is purely presentational.
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
  // Accessibility wiring: glyphs swap (Unicode ⇄ ASCII), allowIcons hides
  // the per-row status icon entirely (replaced with a single space so column
  // alignment doesn't shift). Mirrors the modern TUI's <ActivityTray />
  // pattern; same hooks, same semantics.
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();

  if (tasks.length === 0) return null;
  // /verbose · Task list off — suppress the entire tray. The toggle is
  // surfaced as a top-level row in the verbose menu (alongside Thinking
  // content) so users who don't want pinned UI above the input can hide
  // it independently of any tool-call filters. Tasks themselves are still
  // tracked in the store; flipping back on re-renders the current state.
  if (!getVerboseDisplay().showTasks) return null;

  const done = tasks.filter((t) => t.status === 'completed').length;
  const remaining = tasks.length - done;

  // Mid-dot separator between header parts. Picked from glyphs so ASCII mode
  // gets `.` instead of `·` — keeps the joiner visible without box-drawing.
  const sep = chalk.dim(` ${glyphs.smallDot} `);

  // ── Collapsed: one-line summary ───────────────────────────────────────────
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

  // ── Expanded: full list ───────────────────────────────────────────────────
  // Auto-follow: the viewport tracks the next pending task. When the list is
  // longer than the viewport, we shift `scrollOffset` so the next pending row
  // sits just inside the top of the visible window. Mirrors what the modern
  // TUI's ActivityTrayExpanded does for the tasks tab.
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

  // Header: bold "tasks (N)" followed by counts + collapse hint. Built as a
  // plain string so the joiners stay aligned regardless of which branches
  // contribute parts.
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

        // Tree connector — treeCorner for the final row, treeBranch for
        // everything else. When the visible window doesn't include the
        // actual final task, every row uses treeBranch (the list continues
        // below). Glyphs degrade to `+--`/`+--` in ASCII mode so the tree
        // structure stays legible without box-drawing.
        const connector =
          isLast && hiddenBelow === 0
            ? glyphs.treeCorner
            : i === visible.length - 1 && hiddenBelow > 0
              ? glyphs.treeBranch
              : isLast
                ? glyphs.treeCorner
                : glyphs.treeBranch;

        // Status icon. Disabling allowIcons (`/settings display`) replaces
        // the icon with a single space so column alignment doesn't shift —
        // matches how the modern TUI's <Icon> component handles the toggle.
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

        // Subject styling: completed → dim + strikethrough, next-up → bold,
        // pending → plain. Strikethrough on completed items matches the
        // modern TUI's behavior so the user gets the same visual cue.
        // Applied per-wrapped-line so each row carries its own SGR
        // open/close — no ANSI sequence ever spans a hard line break.
        const styleLine = (s: string): string => {
          if (task.status === 'completed') return chalk.dim.strikethrough(s);
          if (isNext) return chalk.bold(s);
          return s;
        };

        // Visible-col width of everything before the subject begins on the
        // first line:
        //   '  ' (2) + connector (3) + ' ' (1) + icon (1) + ' ' (1)
        //   + `${task.id}.` (id width + 1) + ' ' (1) = 9 + id width.
        // Continuation lines indent by exactly this amount so wrapped
        // subject text aligns with the start of the first-line subject
        // instead of falling back to col 0 under the terminal's default
        // soft-wrap.
        const prefixWidth = 9 + visibleWidth(task.id);
        const cols = process.stdout.columns ?? 80;
        // Floor at 20 cols so a very narrow terminal doesn't degenerate
        // into one-character-per-line wrap. At that size terminal
        // soft-wrap takes back over for the long single line, which is
        // ugly but at least readable.
        const availWidth = Math.max(20, cols - prefixWidth);
        const wrapped = wrapAtWords(task.subject, availWidth, availWidth);
        const indent = ' '.repeat(prefixWidth);

        // First line carries the full prefix; continuation lines carry
        // only the indent. Joined with '\n' and rendered as a single
        // <Text wrap="overflow"> so twinki passes the embedded newlines
        // through verbatim instead of re-wrapping at col 0.
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
      {hiddenBelow > 0 && <Text>{chalk.dim(`  · ${hiddenBelow} below`)}</Text>}
    </Box>
  );
};
