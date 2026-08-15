import React, { useState, useSyncExternalStore } from 'react';
import { describe, expect, it } from 'vitest';
import { Text } from '../src/components/Text.js';
import { useInput } from '../src/hooks/useInput.js';
import { render } from '../src/reconciler/render.js';
import {
  TestTerminal,
  instancePaints,
  settlePaints,
  waitForPaints,
} from './helpers.js';

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
      const paints = instancePaints(instance);
      await waitForPaints(paints, 1);
      await settlePaints(paints);
      await terminal.flush();
      const rendersBeforeInput = paints.paints();

      terminal.sendInput(input);
      // Settling on the counter rather than the expected total keeps an
      // under-painting regression reportable instead of hanging the wait.
      const rendersAfterInput = await settlePaints(paints);
      await terminal.flush();

      expect(terminal.getViewport().join('\n')).toContain('1:1');
      expect(rendersAfterInput - rendersBeforeInput).toBe(expectedRenders);
      instance.unmount();
    }
  );
});
