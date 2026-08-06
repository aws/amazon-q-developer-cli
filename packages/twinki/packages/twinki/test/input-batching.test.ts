import React, { useState, useSyncExternalStore } from 'react';
import { describe, expect, it } from 'vitest';
import { Text } from '../src/components/Text.js';
import { useInput } from '../src/hooks/useInput.js';
import { render } from '../src/reconciler/render.js';
import { TestTerminal, wait } from './helpers.js';

async function waitForText(
  terminal: TestTerminal,
  text: string,
  timeoutMs = 2000
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await terminal.flush();
    if (terminal.getViewport().join('\n').includes(text)) return;
    await wait(5);
  }
  throw new Error(`Timed out waiting for ${JSON.stringify(text)}`);
}

describe('input update batching', () => {
  it.each([
    {
      name: 'default input',
      coalescePrintableRenders: false,
      input: 'x',
      expectedRenders: 2,
    },
    {
      name: 'opted-in input',
      coalescePrintableRenders: true,
      input: 'x',
      expectedRenders: 1,
    },
    {
      name: 'opted-in navigation input',
      coalescePrintableRenders: true,
      input: '\x1b[A',
      expectedRenders: 2,
    },
  ])(
    '$name produces $expectedRenders renderer pass(es)',
    async ({ coalescePrintableRenders, input, expectedRenders }) => {
      const terminal = new TestTerminal();
      let externalValue = 0;
      const listeners = new Set<() => void>();

      function App() {
        const [localValue, setLocalValue] = useState(0);
        const currentExternalValue = useSyncExternalStore(
          (listener) => {
            listeners.add(listener);
            return () => listeners.delete(listener);
          },
          () => externalValue
        );
        useInput(
          () => {
            setLocalValue((value) => value + 1);
            externalValue++;
            for (const listener of listeners) listener();
          },
          { coalescePrintableRenders }
        );
        return React.createElement(
          Text,
          null,
          `${localValue}:${currentExternalValue}`
        );
      }

      const instance = render(React.createElement(App), {
        terminal,
        exitOnCtrlC: false,
      });
      await wait();
      await terminal.flush();
      const rendersBeforeInput = instance.getMetrics().renderCount;

      terminal.sendInput(input);
      await waitForText(terminal, '1:1');

      expect(instance.getMetrics().renderCount - rendersBeforeInput).toBe(
        expectedRenders
      );
      instance.unmount();
    }
  );
});
