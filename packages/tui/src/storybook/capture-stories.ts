#!/usr/bin/env bun
import * as fs from 'fs';
import * as path from 'path';
import { PtyManager } from '../test-utils/shared/pty-manager.js';
import { stories } from './stories.js';
import type {
  StorybookAssertions,
  StorybookKey,
  StorybookPlayContext,
  StorybookVariant,
  StorybookViewport,
} from './contracts.js';

const DEFAULT_VIEWPORT: StorybookViewport = { columns: 120, rows: 40 };
const DEFAULT_SUITE = 'workflow-monitor';

interface CapturedFrame {
  key: string;
  storyId: string;
  variantId: string;
  label: string;
  viewport: StorybookViewport;
  text: string[];
  html: string;
  status: 'passed' | 'failed';
  error?: string;
}

interface CaptureManifest {
  version: 1;
  suite: string;
  generatedAt: string;
  frames: CapturedFrame[];
}

function option(name: string): string | undefined {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? undefined : process.argv[index + 1];
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function safeFilename(value: string): string {
  return value.replace(/[^a-zA-Z0-9_-]/g, '_');
}

function keySequence(key: StorybookKey): string | number[] {
  switch (key) {
    case 'enter':
      return '\r';
    case 'escape':
      return [0x1b];
    case 'tab':
      return '\t';
    case 'up':
      return '\x1b[A';
    case 'down':
      return '\x1b[B';
    case 'left':
      return '\x1b[D';
    case 'right':
      return '\x1b[C';
    case 'ctrl+x':
      return [0x18];
    case 'ctrl+g':
      return [0x07];
  }
}

function assertionFailures(
  text: readonly string[],
  assertions: StorybookAssertions | undefined
): string[] {
  const screen = text.join('\n');
  const failures: string[] = [];
  for (const expected of assertions?.visible ?? []) {
    if (!screen.includes(expected)) failures.push(`missing "${expected}"`);
  }
  for (const forbidden of assertions?.hidden ?? []) {
    if (screen.includes(forbidden)) {
      failures.push(`unexpected "${forbidden}"`);
    }
  }
  return failures;
}

function readBaseline(directory: string | undefined): CaptureManifest | null {
  if (!directory) return null;
  const manifestPath = path.join(path.resolve(directory), 'manifest.json');
  if (!fs.existsSync(manifestPath)) {
    throw new Error(`Baseline manifest not found: ${manifestPath}`);
  }
  const parsed: unknown = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  if (
    typeof parsed !== 'object' ||
    parsed === null ||
    !('version' in parsed) ||
    parsed.version !== 1 ||
    !('frames' in parsed) ||
    !Array.isArray(parsed.frames)
  ) {
    throw new Error(`Unsupported baseline manifest: ${manifestPath}`);
  }
  return parsed as CaptureManifest;
}

function generateReport(
  manifest: CaptureManifest,
  baseline: CaptureManifest | null
): string {
  const baselineFrames = new Map(
    baseline?.frames.map((frame) => [frame.key, frame]) ?? []
  );
  const passed = manifest.frames.filter(
    (frame) => frame.status === 'passed'
  ).length;
  const failed = manifest.frames.length - passed;

  const sections = manifest.frames
    .map((frame, index) => {
      const reference = baselineFrames.get(frame.key);
      const changed =
        reference !== undefined &&
        reference.text.join('\n') !== frame.text.join('\n');
      const comparison = reference
        ? changed
          ? '<span class="changed">changed from baseline</span>'
          : '<span class="matched">matches baseline</span>'
        : baseline
          ? '<span class="missing">no baseline frame</span>'
          : '';
      const error = frame.error
        ? `<pre class="error">${escapeHtml(frame.error)}</pre>`
        : '';
      const referenceColumn = reference
        ? `<div><h3>Baseline</h3><div class="terminal">${reference.html}</div></div>`
        : '';

      return `<section id="frame-${index + 1}">
  <header>
    <div><strong>${index + 1}. ${escapeHtml(frame.label)}</strong>
      <span class="viewport">${frame.viewport.columns}x${frame.viewport.rows}</span>
    </div>
    <div><span class="${frame.status}">${frame.status}</span>${comparison}</div>
  </header>
  ${error}
  <div class="comparison ${reference ? 'two-column' : ''}">
    <div><h3>Current</h3><div class="terminal">${frame.html}</div></div>
    ${referenceColumn}
  </div>
</section>`;
    })
    .join('\n');

  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Workflow monitor visual certification</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#0d1117;color:#c9d1d9;font-family:system-ui,sans-serif}
nav{position:sticky;top:0;z-index:2;display:flex;align-items:center;gap:16px;padding:12px 18px;background:#161b22;border-bottom:1px solid #30363d}
nav h1{font-size:16px;margin:0}nav span{font-size:13px;color:#8b949e}
main{padding:16px}section{margin:0 0 16px;border:1px solid #30363d;border-radius:6px;overflow:hidden;background:#010409}
section>header{display:flex;justify-content:space-between;gap:12px;padding:10px 12px;background:#161b22;border-bottom:1px solid #30363d}
.viewport,.passed,.failed,.matched,.changed,.missing{margin-left:10px;font-size:12px}.passed,.matched{color:#3fb950}.failed,.changed{color:#f85149}.missing{color:#d29922}.viewport{color:#8b949e}
.comparison{display:grid;grid-template-columns:minmax(0,1fr);gap:1px;background:#30363d}.comparison.two-column{grid-template-columns:repeat(2,minmax(0,1fr))}
.comparison>div{min-width:0;background:#010409}.comparison h3{margin:0;padding:8px 12px;color:#8b949e;font-size:12px;border-bottom:1px solid #21262d}
.terminal{overflow:auto}.terminal pre{width:max-content;min-width:100%;font-size:12px;line-height:1.3}.error{margin:0;padding:10px 12px;color:#f85149;background:#2d1117;white-space:pre-wrap}
@media(max-width:1100px){.comparison.two-column{grid-template-columns:1fr}}
</style>
</head>
<body>
<nav><h1>Workflow monitor visual certification</h1><span>${manifest.frames.length} frames</span><span>${passed} passed</span><span>${failed} failed</span><span>${escapeHtml(manifest.generatedAt)}</span></nav>
<main>${sections}</main>
</body>
</html>`;
}

function standaloneFrameDocument(frameHtml: string): string {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Storybook terminal frame</title>
<style>html,body{margin:0;background:#0d1117}pre{font-size:13px;line-height:1.3}</style>
</head>
<body>${frameHtml}</body>
</html>`;
}

function writeEvidence(
  outputDirectory: string,
  manifest: CaptureManifest,
  baseline: CaptureManifest | null
): void {
  fs.rmSync(outputDirectory, { recursive: true, force: true });
  fs.mkdirSync(outputDirectory, { recursive: true });
  manifest.frames.forEach((frame, index) => {
    const prefix = String(index + 1).padStart(3, '0');
    const filename = `${prefix}-${safeFilename(frame.key)}`;
    fs.writeFileSync(
      path.join(outputDirectory, `${filename}.html`),
      standaloneFrameDocument(frame.html)
    );
    fs.writeFileSync(
      path.join(outputDirectory, `${filename}.txt`),
      frame.text.join('\n')
    );
  });
  fs.writeFileSync(
    path.join(outputDirectory, 'manifest.json'),
    JSON.stringify(manifest, null, 2)
  );
  fs.writeFileSync(
    path.join(outputDirectory, 'index.html'),
    generateReport(manifest, baseline)
  );
}

async function captureVariant(
  storyId: string,
  storyName: string,
  variant: StorybookVariant,
  frames: CapturedFrame[]
): Promise<void> {
  const certification = variant.parameters.certification;
  if (!certification) return;

  const viewport = certification.viewport ?? DEFAULT_VIEWPORT;
  const tuiRoot = path.resolve(import.meta.dir, '../..');
  const runner = path.join(import.meta.dir, 'run-storybook.tsx');
  const pty = new PtyManager({
    width: viewport.columns,
    height: viewport.rows,
    cwd: tuiRoot,
    env: {
      CI: 'true',
      FORCE_COLOR: '3',
      KIRO_STORYBOOK_STORY: storyId,
      KIRO_STORYBOOK_VARIANT: variant.id,
    },
  });
  let captureCount = 0;
  let scenarioError: string | undefined;

  const capture = async (label: string): Promise<void> => {
    await sleep(certification.settleMs ?? 150);
    const text = pty.getVisibleSnapshot();
    const failures = assertionFailures(text, certification.assertions);
    const error = failures.length > 0 ? failures.join('; ') : scenarioError;
    frames.push({
      key: `${storyId}--${variant.id}--${captureCount}`,
      storyId,
      variantId: variant.id,
      label: `${storyName} / ${label}`,
      viewport,
      text,
      html: pty.getSnapshotHtml(),
      status: error ? 'failed' : 'passed',
      ...(error ? { error } : {}),
    });
    captureCount += 1;
  };

  const playContext: StorybookPlayContext = {
    press: async (key) => {
      await pty.sendKeys(keySequence(key));
      await sleep(80);
    },
    type: async (text, options) => {
      const delayMs = options?.delayMs ?? 15;
      for (const character of text) {
        await pty.sendKeys(character);
        if (delayMs > 0) await sleep(delayMs);
      }
      await sleep(80);
    },
    waitFor: (text, timeoutMs) =>
      pty.waitForVisibleText(text, timeoutMs ?? 5000),
    sleep,
    capture,
  };

  try {
    pty.spawn(process.execPath, [runner]);
    await pty.waitForVisibleText(certification.readyText, 10000);
    await sleep(certification.settleMs ?? 150);
    await variant.play?.(playContext);
    if (captureCount === 0) await capture(variant.name);
  } catch (error) {
    scenarioError = error instanceof Error ? error.message : String(error);
    await capture(`${variant.name} - capture failure`);
  } finally {
    pty.kill();
  }
}

async function main(): Promise<void> {
  const suite = option('suite') ?? DEFAULT_SUITE;
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
  const outputDirectory = path.resolve(
    option('out') ??
      path.join(
        import.meta.dir,
        '../../e2e_tests/test-outputs',
        `storybook-${suite}-${timestamp}`
      )
  );
  const baseline = readBaseline(option('baseline'));
  const frames: CapturedFrame[] = [];
  const variants = stories.flatMap((story) =>
    story.variants
      .filter((variant) => variant.parameters.certification?.suite === suite)
      .map((variant) => ({ story, variant }))
  );

  if (variants.length === 0) {
    throw new Error(`No certified Storybook variants found for "${suite}"`);
  }

  for (const { story, variant } of variants) {
    process.stdout.write(`Capturing ${story.name} / ${variant.name}... `);
    await captureVariant(story.id, story.name, variant, frames);
    const latest = frames[frames.length - 1];
    process.stdout.write(`${latest?.status ?? 'failed'}\n`);
  }

  const manifest: CaptureManifest = {
    version: 1,
    suite,
    generatedAt: new Date().toISOString(),
    frames,
  };
  writeEvidence(outputDirectory, manifest, baseline);
  const failures = frames.filter((frame) => frame.status === 'failed');
  console.log(`Report: ${path.join(outputDirectory, 'index.html')}`);
  if (failures.length > 0) {
    throw new Error(`${failures.length} visual certification frame(s) failed`);
  }
}

await main();
