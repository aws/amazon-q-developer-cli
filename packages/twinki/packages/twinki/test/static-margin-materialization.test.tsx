import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { render, Box, Text, Static } from '../src/index.js';
import { TestTerminal, wait } from './helpers.js';

/**
 * A static item's root margin occupies rows when the same element renders in
 * a live column. Committed output must occupy those rows too — dropping them
 * shifts every row below at commit time, so a live->static promotion of
 * byte-identical content still forces a repaint of the whole region.
 */
describe('static commit materializes root margins', () => {
  it('emits blank rows for a static child with marginY', async () => {
    const terminal = new TestTerminal(60, 12);
    const App = () => {
      const [items, setItems] = useState<string[]>(['first']);
      (App as { push?: (s: string) => void }).push = (s: string) =>
        setItems((prev) => [...prev, s]);
      return (
        <Box flexDirection="column">
          <Static items={items}>
            {(item) =>
              item === 'margined' ? (
                <Box key={item} marginTop={1} marginBottom={1}>
                  <Text>{item}</Text>
                </Box>
              ) : (
                <Text key={item}>{item}</Text>
              )
            }
          </Static>
          <Text>FOOTER</Text>
        </Box>
      );
    };
    const inst = render(<App />, { terminal, exitOnCtrlC: false });
    try {
      await wait(30);
      await terminal.flush();
      (App as unknown as { push: (s: string) => void }).push('margined');
      await wait(30);
      await terminal.flush();
      (App as unknown as { push: (s: string) => void }).push('last');
      await wait(30);
      await terminal.flush();

      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        all.push((buf.getLine(i)?.translateToString(true) ?? '').trimEnd());
      }
      const first = all.indexOf('first');
      const margined = all.indexOf('margined');
      const last = all.indexOf('last');
      expect(first).toBeGreaterThanOrEqual(0);
      // One blank row above and below the margined item.
      expect(margined).toBe(first + 2);
      expect(all[margined - 1]).toBe('');
      expect(all[margined + 1]).toBe('');
      expect(last).toBe(margined + 2);
    } finally {
      inst.unmount();
    }
  });

  it('emits margin rows even when the item itself renders zero lines', async () => {
    const terminal = new TestTerminal(60, 12);
    const App = () => {
      const [items, setItems] = useState<string[]>(['first']);
      (App as { push?: (s: string) => void }).push = (s: string) =>
        setItems((prev) => [...prev, s]);
      return (
        <Box flexDirection="column">
          <Static items={items}>
            {(item) =>
              item === 'empty-margined' ? (
                <Box key={item} marginTop={1} marginBottom={1} />
              ) : (
                <Text key={item}>{item}</Text>
              )
            }
          </Static>
          <Text>FOOTER</Text>
        </Box>
      );
    };
    const inst = render(<App />, { terminal, exitOnCtrlC: false });
    try {
      await wait(30);
      await terminal.flush();
      (App as unknown as { push: (s: string) => void }).push('empty-margined');
      await wait(30);
      await terminal.flush();
      (App as unknown as { push: (s: string) => void }).push('last');
      await wait(30);
      await terminal.flush();

      const buf = terminal.xtermBuffer();
      const all: string[] = [];
      for (let i = 0; i < buf.length; i++) {
        all.push((buf.getLine(i)?.translateToString(true) ?? '').trimEnd());
      }
      const first = all.indexOf('first');
      const last = all.indexOf('last');
      // The zero-line item still consumes its two margin rows in the live
      // grid; committed output must keep siblings at the same offsets.
      expect(first).toBeGreaterThanOrEqual(0);
      expect(last).toBe(first + 3);
      expect(all[first + 1]).toBe('');
      expect(all[first + 2]).toBe('');
    } finally {
      inst.unmount();
    }
  });
});
