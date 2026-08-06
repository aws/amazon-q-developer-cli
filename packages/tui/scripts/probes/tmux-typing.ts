#!/usr/bin/env bun
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { spawn, spawnSync } from 'node:child_process';

const args = Object.fromEntries(
  process.argv.slice(2).map((arg) => arg.replace(/^--/, '').split('=', 2))
);
const mode = args.mode ?? 'serial';
if (mode !== 'serial' && mode !== 'burst') {
  throw new Error('--mode must be serial or burst');
}
if (spawnSync('tmux', ['-V']).status !== 0) {
  console.log('tmux is not installed; skipping typing stress probe');
  process.exit(0);
}

const staticRows = Number(args.rows ?? 200);
const text =
  mode === 'burst'
    ? 'abcdefghijklmnopqrstuvwxyz0123456789'.repeat(20)
    : 'the quick brown fox jumps over the lazy dog 012345';
const root = resolve(import.meta.dir, '../../../..');
const tuiRoot = join(root, 'packages/tui');
const dir = mkdtempSync(join(tmpdir(), `kiro-control-typing-${mode}-`));
const fixturePath = join(dir, 'fixture.tsx');
const metricsPath = join(dir, 'metrics.json');
const fixtureLog = join(dir, 'fixture.log');
const socketName = `kiro-typing-${process.pid}-${Date.now()}`;
const url = (path: string) => pathToFileURL(join(tuiRoot, path)).href;
const shellQuote = (value: string) => `'${value.replaceAll("'", `'\\''`)}'`;

function percentile(values: number[], p: number): number {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.max(0, Math.ceil((p / 100) * sorted.length) - 1)] ?? 0;
}

async function waitFor(
  check: () => boolean,
  timeoutMs = 15_000
): Promise<void> {
  const deadline = performance.now() + timeoutMs;
  while (performance.now() < deadline) {
    if (check()) return;
    await Bun.sleep(1);
  }
  throw new Error('timed out waiting for tmux typing fixture');
}

function decodeTmux(value: string): string {
  return value
    .replace(/\\([0-7]{3})/g, (_, octal: string) =>
      String.fromCharCode(Number.parseInt(octal, 8))
    )
    .replace(/\\\\/g, '\\');
}

writeFileSync(
  fixturePath,
  `import React from 'react';
import { writeFileSync } from 'node:fs';
import { PromptInput } from ${JSON.stringify(url('src/components/chat/prompt-bar/PromptInput.tsx'))};
import { AppStoreContext, createAppStore } from ${JSON.stringify(url('src/stores/app-store.ts'))};
import { ThemeProvider } from ${JSON.stringify(url('src/theme/ThemeProvider.tsx'))};
import { GlyphsProvider } from ${JSON.stringify(url('src/hooks/useGlyphs.ts'))};
import { Box, Static, Text, render } from ${JSON.stringify(url('src/renderer.ts'))};
import { inputMetrics } from ${JSON.stringify(url('src/utils/inputMetrics.ts'))};

const expectedLength = Number(process.env.EXPECTED_LENGTH);
const staticRows = Number(process.env.STATIC_ROWS);
const store = createAppStore({ kiro: {}, agentEngine: 'kas' });
inputMetrics.enable();
const rows = Array.from({ length: staticRows }, (_, i) => i);
const instance = render(
  <AppStoreContext.Provider value={store}>
    <ThemeProvider theme="auto">
      <GlyphsProvider>
        <Box flexDirection="column">
          <Static items={rows}>
            {(i) => <Text key={i}>conversation row {String(i).padStart(5, '0')}</Text>}
          </Static>
          <PromptInput onSubmit={() => {}} isProcessing={false} />
        </Box>
      </GlyphsProvider>
    </ThemeProvider>
  </AppStoreContext.Provider>
);
const timer = setInterval(() => {
  const value = store.getState().commandInputValue;
  if (value.length !== expectedLength) return;
  clearInterval(timer);
  setTimeout(() => {
    writeFileSync(process.env.METRICS_PATH, JSON.stringify({
      value,
      samples: inputMetrics.getSamples(),
      render: instance.getMetrics(),
    }));
  }, 50);
}, 2);
`
);

const command = [
  `METRICS_PATH=${shellQuote(metricsPath)}`,
  `EXPECTED_LENGTH=${text.length}`,
  `STATIC_ROWS=${staticRows}`,
  `KIRO_HOME=${shellQuote(dir)}`,
  'KIRO_TERMINAL_THEME=safe',
  'KIRO_DISABLE_TELEMETRY=1',
  'bun',
  shellQuote(fixturePath),
  `2>${shellQuote(fixtureLog)}`,
].join(' ');
const child = spawn(
  'tmux',
  [
    '-L',
    socketName,
    '-C',
    '-f',
    '/dev/null',
    'new-session',
    '-s',
    'prompt',
    '-x',
    '120',
    '-y',
    '40',
    command,
  ],
  { stdio: ['pipe', 'pipe', 'pipe'] }
);

let decodedOutput = '';
let controlRemainder = '';
let controlBytes = 0;
child.stdout.setEncoding('utf8');
child.stdout.on('data', (chunk: string) => {
  controlRemainder += chunk;
  let newline: number;
  while ((newline = controlRemainder.indexOf('\n')) !== -1) {
    const line = controlRemainder.slice(0, newline);
    controlRemainder = controlRemainder.slice(newline + 1);
    if (!line.startsWith('%output ')) continue;
    const secondSpace = line.indexOf(' ', 8);
    if (secondSpace === -1) continue;
    const output = decodeTmux(line.slice(secondSpace + 1));
    decodedOutput += output;
    controlBytes += Buffer.byteLength(output);
  }
});
let stderr = '';
child.stderr.setEncoding('utf8');
child.stderr.on('data', (chunk: string) => {
  stderr += chunk;
});
const sendHex = (char: string) => {
  const hex = char.charCodeAt(0).toString(16).padStart(2, '0');
  child.stdin.write(`send-keys -t %0 -H ${hex}\n`);
};

interface FixtureMetrics {
  value: string;
  samples: Array<{ totalLatency: number }>;
  render: { renderCount: number };
}

try {
  await waitFor(() => decodedOutput.includes('ask a question'));
  await Bun.sleep(250);
  const bytesBefore = controlBytes;
  const latencies: number[] = [];
  const started = performance.now();

  if (mode === 'serial') {
    let prefix = '';
    for (const char of text) {
      prefix += char;
      const keyStarted = performance.now();
      sendHex(char);
      await waitFor(() => decodedOutput.includes(prefix));
      latencies.push(performance.now() - keyStarted);
    }
  } else {
    child.stdin.write(
      [...text]
        .map((char) => {
          const hex = char.charCodeAt(0).toString(16).padStart(2, '0');
          return `send-keys -t %0 -H ${hex}`;
        })
        .join('\n') + '\n'
    );
  }

  await waitFor(() => existsSync(metricsPath));
  const elapsedMs = performance.now() - started;
  const metrics = JSON.parse(
    readFileSync(metricsPath, 'utf8')
  ) as FixtureMetrics;
  const totals = metrics.samples.map((sample) => sample.totalLatency);
  const outputBytes = controlBytes - bytesBefore;
  const result = {
    mode,
    staticRows,
    chars: text.length,
    valueMatches: metrics.value === text,
    elapsedMs,
    charsPerSec: text.length / (elapsedMs / 1000),
    bytesPerChar: outputBytes / text.length,
    externalP95: mode === 'serial' ? percentile(latencies, 95) : null,
    internalP95: totals.length ? percentile(totals, 95) : null,
    renderCount: metrics.render.renderCount,
  };
  const passed =
    result.valueMatches &&
    (mode === 'serial'
      ? result.externalP95! < 20 &&
        result.renderCount <= text.length + 2 &&
        result.bytesPerChar < 65
      : result.charsPerSec > 1_000 && result.renderCount <= 5);

  console.log(JSON.stringify({ ...result, passed }, null, 2));
  if (!passed) process.exitCode = 1;
} catch (error) {
  console.error(String(error));
  console.error(stderr);
  if (existsSync(fixtureLog)) {
    console.error(readFileSync(fixtureLog, 'utf8'));
  }
  process.exitCode = 1;
} finally {
  child.stdin.write('kill-server\n');
  child.stdin.end();
  await Promise.race([
    new Promise((resolveExit) => child.once('exit', resolveExit)),
    Bun.sleep(1000),
  ]);
  rmSync(dir, { recursive: true, force: true });
}
