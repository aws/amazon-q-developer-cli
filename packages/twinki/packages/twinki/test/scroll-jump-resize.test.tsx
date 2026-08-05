import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * Characterization of the width-change redraw path: a resize still clears
 * scrollback and re-emits the full frame, because a width change re-wraps
 * every line and the old scrollback content would be mis-wrapped. Streaming
 * full redraws, by contrast, must preserve scrollback.
 */
describe('scrollback wipe on full redraw (native scrollback)', () => {
  it('resizing the terminal emits CLEAR_ALL (\\x1b[3J + \\x1b[H), wiping scrolled-up history', async () => {
    const terminal = new TestTerminal(40, 12);

    const raw: string[] = [];
    const origWrite = terminal.write.bind(terminal);
    (terminal as unknown as { write: (d: string) => void }).write = (d: string) => {
      raw.push(d);
      return origWrite(d);
    };

    // A screenful+ of finalized history in scrollback, plus a small live region.
    const history = Array.from({ length: 40 }, (_, i) => `HIST-${i}`);
    const App = () => {
      const [n] = useState(1);
      return (
        <Box flexDirection="column">
          <Static items={history}>{(h) => <Text>{h}</Text>}</Static>
          <Box flexDirection="column">
            {Array.from({ length: n }, (_, i) => (
              <Text key={i}>{`LIVE-${i}`}</Text>
            ))}
          </Box>
        </Box>
      );
    };

    const inst = render(<App />, {
      terminal,
      exitOnCtrlC: false,
      preserveScrollbackOnRedraw: true,
    });
    try {
      await wait(40);
      await terminal.flush();

      raw.length = 0;
      // A one-column resize — the minimal change a terminal scrollbar or SSH
      // reflow can produce.
      terminal.resize(39, 12);
      await wait(50);
      await terminal.flush();

      const emitted = raw.join('');
      expect(emitted.includes('\x1b[3J')).toBe(true); // scrollback cleared
      expect(emitted.includes('\x1b[H')).toBe(true); // cursor homed → top
    } finally {
      inst.unmount();
    }
  });
});
