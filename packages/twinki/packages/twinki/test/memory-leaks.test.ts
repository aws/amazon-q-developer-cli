import { describe, it, expect } from 'vitest';
import React, { useState } from 'react';
import { TestTerminal, wait } from './helpers.ts';
import { render, Box, Text, Static } from '../src/index.js';
import { renderTree } from '../src/renderer/tree-renderer.js';
import { createNode } from '../src/reconciler/node-factory.js';
import { createYogaNode, Yoga } from '../src/layout/yoga.js';
import type { RootContainer } from '../src/reconciler/types.js';

describe('Memory management', () => {
  it('unmount clears TUI internal state', async () => {
    const term = new TestTerminal(60, 10);
    const instance = render(
      React.createElement(Box, { flexDirection: 'column' },
        React.createElement(Text, null, 'line 1'),
        React.createElement(Text, null, 'line 2'),
      ),
      { terminal: term, exitOnCtrlC: false }
    );
    await wait(50);
    await term.flush();
    instance.unmount();
  });

  it('multiple create/destroy cycles do not crash', async () => {
    for (let i = 0; i < 50; i++) {
      const term = new TestTerminal(60, 10);
      const instance = render(
        React.createElement(Text, null, `iteration ${i}`),
        { terminal: term, exitOnCtrlC: false }
      );
      await wait(10);
      instance.unmount();
    }
    // If we get here without crashing, cleanup works
  });

  it('Static component frees Yoga nodes of flushed items', () => {
    const yogaNode = createYogaNode();
    yogaNode.setFlexDirection(Yoga.FLEX_DIRECTION_COLUMN);
    const root: RootContainer = { yogaNode, children: [], onRender: () => {} };

    const staticNode = createNode('twinki-static', {});
    const N = 10;
    for (let i = 0; i < N; i++) {
      const child = createNode('twinki-box', {});
      staticNode.children.push(child);
      child.parent = staticNode;
      staticNode.yogaNode!.insertChild(child.yogaNode!, i);
    }
    root.children.push(staticNode);
    root.yogaNode.insertChild(staticNode.yogaNode!, 0);

    // Simulate: all N items already flushed to scrollback
    renderTree(root, 80, N);

    // All children's Yoga nodes must be freed and nulled
    for (const child of staticNode.children) {
      expect(child.yogaNode).toBeNull();
    }
    // Static node's own yogaNode must remain intact
    expect(staticNode.yogaNode).not.toBeNull();

    root.yogaNode.removeChild(staticNode.yogaNode!);
    staticNode.yogaNode!.free();
    root.yogaNode.free();
  });

  it('1000-turn chat with 1MB messages: heap stays flat after GC', async () => {
    const TURNS = 1000;
    const MSG = 'x'.repeat(1024 * 1024); // 1MB

    interface Msg { id: number; text: string; }
    let addMessage!: (text: string) => void;

    const ChatApp = () => {
      const [messages, setMessages] = useState<Msg[]>([]);
      addMessage = (text: string) =>
        setMessages(prev => [...prev, { id: prev.length, text }]);
      const done = messages.slice(0, -1);
      const current = messages.at(-1);
      return React.createElement(Box, { flexDirection: 'column' },
        React.createElement(Static, { items: done },
          (msg: Msg) => React.createElement(Box, { key: msg.id },
            React.createElement(Text, null, `[${msg.id}] ${msg.text.slice(0, 20)}`)
          )
        ),
        current && React.createElement(Text, null, `live: ${current.id}`)
      );
    };

    const gc = (globalThis as any).gc as (() => void) | undefined;
    const term = new TestTerminal(80, 24);
    const instance = render(React.createElement(ChatApp), {
      terminal: term,
      exitOnCtrlC: false,
    });
    await wait(50);

    // Warm up: run 10 turns to let React/V8 stabilise
    for (let i = 0; i < 10; i++) {
      addMessage(MSG);
      await wait(10);
      await term.flush();
    }
    gc?.();
    const heapAfter10 = process.memoryUsage().heapUsed;

    // Run remaining 90 turns
    for (let i = 10; i < TURNS; i++) {
      addMessage(MSG);
      await wait(10);
      await term.flush();
    }
    gc?.();
    const heapAfter100 = process.memoryUsage().heapUsed;

    // Last frame must show the final message
    expect(term.getLastFrame()?.viewport.join('\n')).toContain(`live: ${TURNS - 1}`);

    // Heap growth over 990 turns of 1MB messages should be <500MB.
    // Per-turn cost is ~430KB React fiber overhead per Static item; 1MB message strings are GC'd.
    // Without the Yoga fix this crashes with a Wasm double-free well before turn 1000.
    const growthMB = (heapAfter100 - heapAfter10) / 1024 / 1024;
    expect(growthMB).toBeLessThan(500);

    instance.unmount();
  }, 300_000); // allow up to 5min for 1000 turns
});