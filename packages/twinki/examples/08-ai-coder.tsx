/**
 * 08-ai-coder.tsx — AI Coding Assistant
 *
 * Run: npx tsx examples/08-ai-coder.tsx
 *
 * Scripted chat simulating an AI building a React app step by step.
 * Demonstrates all content types together:
 *   - Markdown prose with bold/italic/lists
 *   - Syntax-highlighted code blocks
 *   - DiffView tool results (file edits)
 *   - Fast typewriter streaming
 *
 * Controls:
 *   Space / Enter   advance to next message
 *   q / Ctrl+C      quit
 */
import React, { useState, useEffect, useRef } from 'react';
import { render, Text, Box, Markdown, DiffView, useInput, useApp, getHighlighter } from 'twinki';

// Preload themes so diff switching is instant
const THEMES = ['github-dark', 'monokai', 'nord', 'tokyo-night'] as const;
type Theme = typeof THEMES[number];
Promise.all(THEMES.map(t => getHighlighter(t, 'typescript'))).catch(() => {});

// ── Script ────────────────────────────────────────────────────────────────────

type MsgKind = 'user' | 'text' | 'diff';

interface UserMsg  { kind: 'user';  text: string }
interface TextMsg  { kind: 'text';  text: string }
interface DiffMsg  { kind: 'diff';  file: string; old: string; newCode: string; lang: string }

type ScriptMsg = UserMsg | TextMsg | DiffMsg;

const COUNTER_V1 = `import React, { useState } from 'react';

export function Counter() {
  const [count, setCount] = useState(0);
  return (
    <div>
      <p>Count: {count}</p>
      <button onClick={() => setCount(count + 1)}>+</button>
    </div>
  );
}`;

const COUNTER_V2 = `import React, { useState, useCallback } from 'react';

interface CounterProps {
  initial?: number;
  step?: number;
}

export function Counter({ initial = 0, step = 1 }: CounterProps) {
  const [count, setCount] = useState(initial);
  const increment = useCallback(() => setCount(c => c + step), [step]);
  const decrement = useCallback(() => setCount(c => c - step), [step]);
  const reset = useCallback(() => setCount(initial), [initial]);
  return (
    <div className="counter">
      <p>Count: <strong>{count}</strong></p>
      <button onClick={decrement}>−</button>
      <button onClick={increment}>+</button>
      <button onClick={reset}>Reset</button>
    </div>
  );
}`;

const APP_V1 = `import React from 'react';
import { Counter } from './Counter';

export default function App() {
  return (
    <div>
      <h1>My App</h1>
      <Counter />
    </div>
  );
}`;

const APP_V2 = `import React from 'react';
import { Counter } from './Counter';

export default function App() {
  return (
    <div className="app">
      <h1>My App</h1>
      <p>A demo of the Counter component with custom props.</p>
      <Counter initial={10} step={5} />
      <Counter initial={0} step={1} />
    </div>
  );
}`;

const HOOK_NEW = `import { useState, useCallback } from 'react';

export function useCounter(initial = 0, step = 1) {
  const [count, setCount] = useState(initial);
  const increment = useCallback(() => setCount(c => c + step), [step]);
  const decrement = useCallback(() => setCount(c => c - step), [step]);
  const reset = useCallback(() => setCount(initial), [initial]);
  return { count, increment, decrement, reset };
}`;

const COUNTER_V3 = `import React from 'react';
import { useCounter } from './useCounter';

interface CounterProps {
  initial?: number;
  step?: number;
}

export function Counter({ initial = 0, step = 1 }: CounterProps) {
  const { count, increment, decrement, reset } = useCounter(initial, step);
  return (
    <div className="counter">
      <p>Count: <strong>{count}</strong></p>
      <button onClick={decrement}>−</button>
      <button onClick={increment}>+</button>
      <button onClick={reset}>Reset</button>
    </div>
  );
}`;

const SCRIPT: ScriptMsg[] = [
  { kind: 'user', text: 'Build me a counter component in React with TypeScript.' },
  { kind: 'text', text: `Sure! I'll start with a simple **Counter** component.

It will have:
- A \`count\` state initialized to \`0\`
- An increment button
- Clean TypeScript types` },
  { kind: 'diff', file: 'src/Counter.tsx', old: '', newCode: COUNTER_V1, lang: 'tsx' },
  { kind: 'text', text: `That's the basic version. Want me to add **decrement**, **reset**, and configurable \`step\`/\`initial\` props?` },

  { kind: 'user', text: 'Yes, and also update App.tsx to use it with some custom props.' },
  { kind: 'text', text: `I'll refactor \`Counter\` to accept props, then update \`App\` to render two instances with different configs.` },
  { kind: 'diff', file: 'src/Counter.tsx', old: COUNTER_V1, newCode: COUNTER_V2, lang: 'tsx' },
  { kind: 'diff', file: 'src/App.tsx', old: APP_V1, newCode: APP_V2, lang: 'tsx' },
  { kind: 'text', text: `Done. The two counters are independent — one starts at **10** with step **5**, the other is the default.` },

  { kind: 'user', text: 'Extract the counter logic into a custom hook.' },
  { kind: 'text', text: `Good call. I'll extract into \`useCounter\`, then simplify the component to just call the hook.

This separates concerns:
- **\`useCounter\`** owns all state logic
- **\`Counter\`** is a pure presentation component` },
  { kind: 'diff', file: 'src/useCounter.ts', old: '', newCode: HOOK_NEW, lang: 'typescript' },
  { kind: 'diff', file: 'src/Counter.tsx', old: COUNTER_V2, newCode: COUNTER_V3, lang: 'tsx' },
  { kind: 'text', text: `All done! Here's the final structure:

\`\`\`
src/
  useCounter.ts   — custom hook (state logic)
  Counter.tsx     — component (presentation only)
  App.tsx         — composes two Counter instances
\`\`\`

The hook is also independently testable — you can use it in any component without the UI.` },
];

// ── Typewriter ────────────────────────────────────────────────────────────────

function useTypewriter(target: string, speed = 8): string {
  const [displayed, setDisplayed] = useState('');
  const ref = useRef(target);
  useEffect(() => {
    ref.current = target;
    setDisplayed('');
  }, [target]);
  useEffect(() => {
    if (displayed.length >= ref.current.length) return;
    const t = setTimeout(() => {
      setDisplayed(ref.current.slice(0, displayed.length + speed));
    }, 16);
    return () => clearTimeout(t);
  }, [displayed, speed]);
  return displayed;
}

function closeOpenFences(text: string): string {
  return (text.match(/^```/gm) || []).length % 2 === 1 ? text + '\n```' : text;
}

// ── Message components ────────────────────────────────────────────────────────

const UserBubble = ({ text }: { text: string }) => (
  <Box flexDirection="column" marginBottom={1}>
    <Text color="cyan" bold>  You</Text>
    <Text>  {text}</Text>
  </Box>
);

const StreamingText = ({ text, theme }: { text: string; theme: Theme }) => {
  const displayed = useTypewriter(text, 6);
  const done = displayed.length >= text.length;
  return (
    <Box flexDirection="column" marginBottom={1}>
      <Text color="green" bold>  AI{done ? '' : ' ✦'}</Text>
      <Box paddingLeft={2}>
        <Markdown theme={theme}>{closeOpenFences(displayed)}</Markdown>
      </Box>
    </Box>
  );
};

const DiffResult = ({ file, old: oldCode, newCode, lang, theme }: {
  file: string; old: string; newCode: string; lang: string; theme: Theme;
}) => (
  <Box flexDirection="column" marginBottom={1}>
    <Text color="yellow" bold>  ⟳ edit </Text>
    <Text dimColor>  {file}</Text>
    <Box paddingLeft={2} paddingTop={1}>
      <DiffView
        values={[oldCode, newCode]}
        layout="vertical"
        highlight
        lang={lang}
        theme={theme}
      />
    </Box>
  </Box>
);

// ── App ───────────────────────────────────────────────────────────────────────

const App = () => {
  const [cursor, setCursor] = useState(0);   // how many script items are visible
  const [themeIdx, setThemeIdx] = useState(0);
  const { exit } = useApp();
  const theme = THEMES[themeIdx]!;

  // Auto-advance through script
  const advance = () => setCursor(c => Math.min(c + 1, SCRIPT.length));

  // Auto-play: advance when current streaming text finishes
  const current = SCRIPT[cursor - 1];
  const currentText = current?.kind === 'text' ? current.text : null;
  const [streamDone, setStreamDone] = useState(false);

  useEffect(() => { setStreamDone(false); }, [cursor]);

  useInput((ch, key) => {
    if (ch === 'q' || (key.ctrl && ch === 'c')) exit();
    if (ch === 't') setThemeIdx(i => (i + 1) % THEMES.length);
    if (key.return || ch === ' ') advance();
  });

  const visible = SCRIPT.slice(0, cursor);

  return (
    <Box flexDirection="column">
      {/* Header */}
      <Box borderStyle="round" borderColor="cyan" paddingX={1} flexDirection="row">
        <Text>
          <Text bold color="cyan">AI Coder  </Text>
          <Text dimColor>theme=</Text>
          <Text color="magenta">{theme}</Text>
          <Text dimColor>  Space/Enter advance  •  t theme  •  q quit</Text>
        </Text>
      </Box>

      <Text> </Text>

      {/* Rendered messages */}
      {visible.map((msg, i) => {
        if (msg.kind === 'user') return <UserBubble key={i} text={msg.text} />;
        if (msg.kind === 'text') return <StreamingText key={i} text={msg.text} theme={theme} />;
        return <DiffResult key={i} {...msg} theme={theme} />;
      })}

      {/* Prompt when waiting */}
      {cursor < SCRIPT.length && (
        <Text dimColor>  ↵ next</Text>
      )}
      {cursor >= SCRIPT.length && (
        <Text dimColor>  — end of script —</Text>
      )}
    </Box>
  );
};

render(<App />);
