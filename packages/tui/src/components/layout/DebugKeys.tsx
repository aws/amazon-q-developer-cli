/**
 * Diagnostic mode for keyboard input. Activated with `--debug-keys`.
 *
 * Prints every key event with raw bytes (hex), the parsed key id, and the
 * Key flags so we can see exactly what the terminal is sending and what
 * twinki/our parser is making of it. Useful when a shortcut isn't working
 * and we need to know whether the bytes never arrived, arrived but were
 * misclassified, or arrived but the dispatch table doesn't handle them.
 *
 * Press Esc twice within 1 second to exit.
 */
import React, { useEffect, useRef, useState } from 'react';
import { Box, Text, Static } from '../../renderer.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import chalk from 'chalk';

interface KeyEvent {
  id: number;
  raw: string;
  rawHex: string;
  input: string;
  parsed: string;
}

let nextId = 0;

export const DebugKeys: React.FC = () => {
  const [events, setEvents] = useState<KeyEvent[]>([]);
  const lastEscRef = useRef(0);

  useKeypress((input, key) => {
    // Esc-Esc within 1s exits.
    if (key.escape) {
      const now = Date.now();
      if (now - lastEscRef.current < 1000) {
        process.stdout.write('\n');
        process.exit(0);
      }
      lastEscRef.current = now;
    }
    // Re-encode to a likely-raw byte string for display. The handler doesn't
    // see the original bytes (twinki parses them upstream), so we reconstruct
    // a plausible representation from the Key flags + input. For most keys
    // input is the literal char; for control keys we reconstruct from flags.
    const raw = key.escape
      ? '\x1b'
      : key.return
        ? '\r'
        : key.backspace
          ? '\x7f'
          : key.tab
            ? '\t'
            : key.delete
              ? '\x1b[3~'
              : key.upArrow
                ? '\x1b[A'
                : key.downArrow
                  ? '\x1b[B'
                  : key.leftArrow
                    ? '\x1b[D'
                    : key.rightArrow
                      ? '\x1b[C'
                      : input;

    const flags: string[] = [];
    if (key.ctrl) flags.push('ctrl');
    if (key.meta) flags.push('meta/alt');
    if (key.shift) flags.push('shift');
    if (key.escape) flags.push('escape');
    if (key.return) flags.push('return');
    if (key.backspace) flags.push('backspace');
    if (key.delete) flags.push('delete');
    if (key.tab) flags.push('tab');
    if (key.upArrow) flags.push('up');
    if (key.downArrow) flags.push('down');
    if (key.leftArrow) flags.push('left');
    if (key.rightArrow) flags.push('right');
    if (key.home) flags.push('home');
    if (key.end) flags.push('end');
    if (key.pageUp) flags.push('pageUp');
    if (key.pageDown) flags.push('pageDown');
    if (key.paste) flags.push('paste');

    const ev: KeyEvent = {
      id: nextId++,
      raw,
      rawHex: toHex(raw),
      input,
      parsed: flags.length > 0 ? flags.join('+') : input || '(empty)',
    };
    setEvents((prev) => [...prev, ev]);
  });

  // Limit how much we keep in memory (Static items grow without bound).
  const visible = events.slice(-200);

  return (
    <Box flexDirection="column">
      <Static items={visible}>
        {(ev) => (
          <Text key={ev.id}>
            {chalk.dim(`#${String(ev.id).padStart(4)}`)} raw=
            {chalk.cyan(ev.rawHex)} input=
            {chalk.yellow(JSON.stringify(ev.input))} parsed=
            {chalk.green(ev.parsed)}
          </Text>
        )}
      </Static>
      <Box marginTop={1}>
        <Text>{chalk.dim('press any key — Esc Esc within 1s to exit')}</Text>
      </Box>
    </Box>
  );
};

function toHex(s: string): string {
  if (!s) return '∅';
  return [...s]
    .map((c) => {
      const code = c.charCodeAt(0);
      if (code < 0x20 || code === 0x7f)
        return `\\x${code.toString(16).padStart(2, '0')}`;
      return c;
    })
    .join('');
}
