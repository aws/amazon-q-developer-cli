#!/usr/bin/env bun
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { PtyManager } from '../test-utils/shared/pty-manager.js';
import {
  compareTerminalFrames,
  parseTerminalFrame,
  renderTerminalFrameHtml,
  serializeDiagnosticTextFrame,
  serializeTerminalFrame,
  terminalFrameText,
  type SerializedTerminalFrame,
} from '../test-utils/shared/terminal-frame.js';
import { stories } from './stories.js';
import {
  collectVisualCoverage,
  isVariantSelectedForVisualSuite,
  STORYBOOK_CATALOG_SUITE,
  visualCoverageHtml,
  visualCoverageMarkdown,
  visualCoverageSummaryMarkdown,
  type VisualCoverage,
} from './visual-coverage.js';
import {
  mergeStoryAssertions,
  missingCaptureIds,
  rendererFailureMessages,
  resolveCaptureDefinition,
  validateCaptureId,
} from './story-capture-contract.js';
import { finalizeVisualEvidence } from './visual-evidence.js';
import type {
  StorybookAssertions,
  StorybookKey,
  StorybookPlayContext,
  StorybookVariant,
  StorybookViewport,
} from './contracts.js';

const DEFAULT_VIEWPORT: StorybookViewport = { columns: 120, rows: 40 };
const DEFAULT_SETTLE_MS = 150;
const DEFAULT_SUITE = STORYBOOK_CATALOG_SUITE;

interface CapturedFrame {
  key: string;
  storyId: string;
  storyName: string;
  variantId: string;
  variantName: string;
  captureId: string;
  captureLabel: string;
  label: string;
  frame: SerializedTerminalFrame;
  status: 'passed' | 'failed';
  error?: string;
}

interface CaptureManifest {
  version: 2;
  suite: string;
  generatedAt: string;
  coverage: VisualCoverage;
  frames: CapturedFrame[];
  evidenceErrors?: string[];
}

function option(name: string): string | undefined {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? undefined : process.argv[index + 1];
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function waitForRenderedFrame(
  pty: PtyManager,
  timeoutMs = 10000
): Promise<void> {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    if (pty.getVisibleSnapshot().some((line) => line.trim().length > 0)) return;
    await sleep(50);
  }
  throw new Error('Timed out waiting for the story to render visible content');
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
    case 'shift+up':
      return '\x1b[1;2A';
    case 'shift+down':
      return '\x1b[1;2B';
    case 'shift+left':
      return '\x1b[1;2D';
    case 'shift+right':
      return '\x1b[1;2C';
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
  let orderedOffset = 0;
  for (const expected of assertions?.ordered ?? []) {
    const index = screen.indexOf(expected, orderedOffset);
    if (index === -1) {
      failures.push(`missing ordered "${expected}"`);
      break;
    }
    orderedOffset = index + expected.length;
  }
  for (const [expected, count] of Object.entries(
    assertions?.occurrences ?? {}
  )) {
    let actual = 0;
    let offset = 0;
    while (expected.length > 0) {
      const index = screen.indexOf(expected, offset);
      if (index === -1) break;
      actual += 1;
      offset = index + expected.length;
    }
    if (actual !== count) {
      failures.push(
        `expected "${expected}" ${count} time${count === 1 ? '' : 's'}, found ${actual}`
      );
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
    parsed.version !== 2 ||
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
  const reportFrames = manifest.frames.map((frame, index) => {
    const reference = baselineFrames.get(frame.key);
    const currentFrame = parseTerminalFrame(frame.frame);
    const referenceFrame = reference
      ? parseTerminalFrame(reference.frame)
      : undefined;
    const diff = referenceFrame
      ? compareTerminalFrames(referenceFrame, currentFrame)
      : undefined;
    const comparisonState =
      frame.status === 'failed'
        ? 'failed'
        : diff && !diff.equal
          ? 'changed'
          : reference
            ? 'matched'
            : baseline
              ? 'new'
              : 'passed';
    const comparisonText =
      comparisonState === 'changed'
        ? `${String(diff?.changedCells ?? 0)} cells changed`
        : comparisonState === 'matched'
          ? 'baseline match'
          : comparisonState === 'new'
            ? 'new frame'
            : comparisonState;
    return {
      frame,
      index,
      currentFrame,
      referenceFrame,
      comparisonState,
      comparisonText,
    };
  });
  const changed = reportFrames.filter(
    ({ comparisonState }) => comparisonState === 'changed'
  ).length;
  const matched = reportFrames.filter(
    ({ comparisonState }) => comparisonState === 'matched'
  ).length;
  const newFrames = reportFrames.filter(
    ({ comparisonState }) => comparisonState === 'new'
  ).length;
  const evidenceErrors =
    manifest.evidenceErrors?.length === 0 || !manifest.evidenceErrors
      ? ''
      : `<section class="evidence-errors" role="alert"><strong>Evidence is incomplete.</strong><ul>${manifest.evidenceErrors
          .map((error) => `<li>${escapeHtml(error)}</li>`)
          .join('')}</ul></section>`;
  const framesByStory = new Map<string, typeof reportFrames>();
  for (const reportFrame of reportFrames) {
    const storyFrames = framesByStory.get(reportFrame.frame.storyName) ?? [];
    storyFrames.push(reportFrame);
    framesByStory.set(reportFrame.frame.storyName, storyFrames);
  }

  const navigation = [...framesByStory.entries()]
    .map(
      (
        [storyName, storyFrames],
        storyIndex
      ) => `<details class="story-group"${storyIndex === 0 ? ' open' : ''}>
  <summary><span>${escapeHtml(storyName)}</span><span class="story-count">${storyFrames.length}</span></summary>
  <ul>
    ${storyFrames
      .map(
        ({ frame, index, comparisonState }) => `<li>
      <button type="button" class="frame-link${index === 0 ? ' active' : ''}" data-frame="${index}" data-state="${comparisonState}" data-search="${escapeHtml(`${storyName} ${frame.variantName} ${frame.captureId} ${frame.captureLabel}`.toLowerCase())}" aria-controls="frame-${index + 1}"${index === 0 ? ' aria-current="page"' : ''}>
        <span class="variant-name">${escapeHtml(frame.variantName)}</span>
        <span class="capture-name">${escapeHtml(frame.captureId === 'default' ? 'static frame' : frame.captureLabel)}</span>
        <span class="state-mark ${comparisonState}">${comparisonState}</span>
      </button>
    </li>`
      )
      .join('')}
  </ul>
</details>`
    )
    .join('\n');

  const sections = reportFrames
    .map(
      ({
        frame,
        index,
        currentFrame,
        referenceFrame,
        comparisonState,
        comparisonText,
      }) => {
        const error = frame.error
          ? `<pre class="error">${escapeHtml(frame.error)}</pre>`
          : '';
        const referenceColumn = referenceFrame
          ? `<figure><figcaption>Baseline</figcaption><div class="terminal">${renderTerminalFrameHtml(referenceFrame)}</div></figure>`
          : '';

        return `<article class="frame-panel" id="frame-${index + 1}" data-frame-panel="${index}"${index === 0 ? '' : ' hidden'}>
  <header class="frame-header">
    <div>
      <p class="frame-path">${escapeHtml(frame.storyId)} / ${escapeHtml(frame.variantId)}</p>
      <h2>${escapeHtml(frame.storyName)} <span>/ ${escapeHtml(frame.variantName)}</span></h2>
      <p class="capture-description">${escapeHtml(frame.captureLabel)} <code>#${escapeHtml(frame.captureId)}</code></p>
    </div>
    <dl class="frame-facts">
      <div><dt>Viewport</dt><dd>${currentFrame.viewport.columns} × ${currentFrame.viewport.rows}</dd></div>
      <div><dt>Assertion</dt><dd class="${frame.status}">${frame.status}</dd></div>
      <div><dt>Comparison</dt><dd class="${comparisonState}">${comparisonText}</dd></div>
    </dl>
  </header>
  ${error}
  <div class="comparison ${referenceFrame ? 'two-column' : ''}">
    <figure><figcaption>Current</figcaption><div class="terminal">${renderTerminalFrameHtml(currentFrame)}</div></figure>
    ${referenceColumn}
  </div>
</article>`;
      }
    )
    .join('\n');

  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<base target="_blank">
<title>Visual stories</title>
<style>
:root{--paper:oklch(94.3% 0 0);--panel:oklch(98% 0 0);--ink:oklch(20% 0 0);--mute:oklch(48% 0 0);--line:oklch(78% 0 0);--accent:oklch(52% .13 55);--ok:oklch(47.5% .1 147.7);--bad:oklch(46.5% .147 24.9);--warn:oklch(56% .12 74);--s1:.25rem;--s2:.5rem;--s3:.75rem;--s4:1rem;--s5:1.5rem;--s6:2rem;--z-sticky:100}
*{box-sizing:border-box}html,body{height:100%;overflow:hidden}body{margin:0;min-height:100svh;background:var(--paper);color:var(--ink);font-family:"Avenir Next",Avenir,Optima,"Trebuchet MS",sans-serif}
button,input{font:inherit}button:focus-visible,input:focus-visible,a:focus-visible,summary:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.masthead{min-height:5.25rem;display:flex;align-items:center;gap:var(--s5);padding:var(--s3) var(--s5);background:var(--ink);color:var(--paper);border-bottom:1px solid var(--ink)}
.identity{min-width:14rem}.identity h1{margin:0;font-family:"Rockwell",Georgia,serif;font-size:clamp(1.25rem,1.1rem + .5vw,1.75rem);line-height:1.1;font-weight:700}.identity p{margin:var(--s1) 0 0;color:oklch(78% 0 0);font:600 .68rem/1.2 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace;letter-spacing:.08em;text-transform:uppercase}
.metrics{display:flex;flex-wrap:wrap;align-items:center;gap:var(--s2) var(--s5);margin:0}.metrics div{display:grid;grid-template-columns:auto auto;align-items:baseline;gap:var(--s2)}.metrics dt{font-size:.72rem;color:oklch(78% 0 0)}.metrics dd{margin:0;font:700 .9rem/1 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace;font-variant-numeric:tabular-nums}
.masthead a{margin-left:auto;color:oklch(82% .1 70);font-size:.8rem;text-underline-offset:3px}
.workbench{height:calc(100dvh - 5.25rem);display:grid;grid-template-columns:minmax(17rem,22rem) minmax(0,1fr)}
.catalog{min-width:0;overflow:auto;background:var(--panel);border-right:1px solid var(--line)}
.catalog-tools{position:sticky;top:0;z-index:var(--z-sticky);padding:var(--s4);background:var(--panel);border-bottom:1px solid var(--line)}
.search-label{display:block;margin-bottom:var(--s2);font-size:.72rem;font-weight:700;letter-spacing:.05em;text-transform:uppercase;color:var(--mute)}
#story-search{width:100%;min-height:2.75rem;padding:var(--s2) var(--s3);border:1px solid var(--line);border-radius:4px;background:var(--paper);color:var(--ink)}
#story-search:hover{border-color:var(--mute)}#story-search::placeholder{color:var(--mute);opacity:1}
.filters{display:grid;grid-template-columns:repeat(3,1fr);gap:1px;margin-top:var(--s3);border:1px solid var(--line);background:var(--line)}
.filter{min-height:2.75rem;border:0;background:var(--panel);color:var(--mute);font-size:.72rem;font-weight:700;cursor:pointer}.filter:hover{color:var(--ink);background:var(--paper)}.filter.active{color:var(--panel);background:var(--accent)}
.story-group{border-bottom:1px solid var(--line)}.story-group[hidden]{display:none}.story-group summary{display:flex;justify-content:space-between;align-items:center;min-height:2.75rem;padding:var(--s2) var(--s4);cursor:pointer;font-size:.78rem;font-weight:700}.story-group summary:hover{background:var(--paper)}
.story-count{font:600 .68rem/1 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace;color:var(--mute)}
.story-group ul{list-style:none;margin:0;padding:0}.story-group li[hidden]{display:none}.frame-link{position:relative;width:100%;min-height:3.25rem;display:grid;grid-template-columns:minmax(0,1fr) auto;gap:var(--s1) var(--s2);padding:var(--s2) var(--s4) var(--s2) var(--s6);border:0;border-top:1px solid color-mix(in oklch,var(--line) 55%,var(--panel));text-align:left;background:var(--panel);color:var(--ink);cursor:pointer}
.frame-link:hover{background:var(--paper)}.frame-link.active{background:color-mix(in oklch,var(--accent) 10%,var(--panel))}.frame-link.active::before{content:"›";position:absolute;left:var(--s3);top:var(--s2);color:var(--accent);font-size:1.1rem;font-weight:700}
.variant-name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:.77rem;font-weight:600}.capture-name{overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font:400 .65rem/1.2 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace;color:var(--mute)}
.state-mark{grid-row:1/3;grid-column:2;align-self:center;font:700 .58rem/1 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace;letter-spacing:.04em;text-transform:uppercase}.passed,.matched{color:var(--ok)}.failed,.changed{color:var(--bad)}.new{color:var(--warn)}
.viewer{min-width:0;overflow:auto;background:oklch(17.7% .009 264.3)}.frame-panel{min-height:100%;background:oklch(17.7% .009 264.3);color:oklch(93.3% .007 260.7)}.frame-panel[hidden]{display:none}
.frame-header{display:flex;justify-content:space-between;align-items:flex-start;gap:var(--s5);padding:var(--s5);border-bottom:1px solid oklch(32% .012 260)}.frame-path{margin:0 0 var(--s2);color:oklch(66% .025 257);font:600 .66rem/1.2 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace;letter-spacing:.04em}.frame-header h2{margin:0;font-family:"Rockwell",Georgia,serif;font-size:clamp(1.1rem,1rem + .4vw,1.5rem);line-height:1.2}.frame-header h2 span{color:oklch(72% .02 257);font-weight:400}.capture-description{margin:var(--s2) 0 0;color:oklch(72% .02 257);font-size:.78rem}.capture-description code{margin-left:var(--s2);color:oklch(82% .1 70);font-family:ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace}
.frame-facts{display:grid;grid-template-columns:repeat(3,auto);gap:var(--s4);margin:0}.frame-facts div{min-width:6rem}.frame-facts dt{margin-bottom:var(--s1);color:oklch(66% .025 257);font-size:.65rem;text-transform:uppercase;letter-spacing:.06em}.frame-facts dd{margin:0;font:700 .7rem/1.2 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace;text-transform:uppercase}
.comparison{display:grid;grid-template-columns:minmax(0,1fr);gap:1px;background:oklch(32% .012 260)}.comparison.two-column{grid-template-columns:repeat(2,minmax(0,1fr))}.comparison figure{min-width:0;margin:0;background:#010409}.comparison figcaption{padding:var(--s2) var(--s4);border-bottom:1px solid #21262d;color:#8b949e;font:700 .66rem/1.2 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace;letter-spacing:.06em;text-transform:uppercase}
.terminal{overflow:auto}.terminal pre{width:max-content;min-width:100%;font-size:12px;line-height:1.3}.error{margin:0;padding:var(--s3) var(--s5);color:oklch(86% .08 25);background:oklch(26% .06 25);white-space:pre-wrap;font:400 .75rem/1.5 ui-monospace,"SF Mono","Cascadia Code",Menlo,Consolas,monospace}
.evidence-errors{padding:var(--s3) var(--s5);color:oklch(25% .08 25);background:oklch(90% .07 55);border-bottom:1px solid oklch(70% .1 45);font-size:.8rem}.evidence-errors ul{margin:var(--s2) 0 0;padding-left:var(--s5)}
.empty{display:none;padding:var(--s5);color:var(--mute);font-size:.8rem}.catalog.no-results .empty{display:block}
@media(max-width:900px){html,body{overflow:auto}.masthead{align-items:flex-start;flex-wrap:wrap}.masthead a{margin-left:0}.workbench{height:auto;min-height:calc(100dvh - 5.25rem);grid-template-columns:1fr}.catalog{max-height:42dvh;border-right:0;border-bottom:1px solid var(--line)}.viewer{min-height:58dvh}.frame-header{flex-direction:column}.comparison.two-column{grid-template-columns:1fr}}
@media(max-width:520px){.masthead{padding:var(--s3) var(--s4)}.identity{min-width:100%}.metrics{gap:var(--s2) var(--s4)}.frame-facts{grid-template-columns:1fr 1fr}.frame-header{padding:var(--s4)}}
@media(prefers-reduced-motion:reduce){*,*::before,*::after{scroll-behavior:auto!important;transition-duration:.01ms!important}}
</style>
</head>
<body>
<header class="masthead">
  <div class="identity"><h1>Visual Stories</h1><p>${escapeHtml(manifest.suite)} · ${escapeHtml(manifest.generatedAt)}</p></div>
  <dl class="metrics">
    <div><dt>Frames</dt><dd>${manifest.frames.length}</dd></div>
    <div><dt>Passed</dt><dd>${passed}</dd></div>
    <div><dt>Failed</dt><dd>${failed}</dd></div>
    <div><dt>Changed</dt><dd>${changed}</dd></div>
    <div><dt>Matched</dt><dd>${matched}</dd></div>
    <div><dt>New</dt><dd>${newFrames}</dd></div>
    <div><dt>Stories</dt><dd>${formatCoverage(manifest.coverage.storyCoveragePercent)}</dd></div>
    <div><dt>Variants</dt><dd>${formatCoverage(manifest.coverage.variantExecutionPercent)}</dd></div>
  </dl>
  <a href="coverage.html">Coverage inventory</a>
</header>
${evidenceErrors}
<div class="workbench">
  <nav class="catalog" aria-label="Story frames">
    <div class="catalog-tools">
      <label class="search-label" for="story-search">Find a story or state</label>
      <input id="story-search" type="search" placeholder="Workflow, loading, error…">
      <div class="filters" aria-label="Frame status filter">
        <button type="button" class="filter active" data-filter="all">All</button>
        <button type="button" class="filter" data-filter="attention">Attention</button>
        <button type="button" class="filter" data-filter="passed">Passed</button>
      </div>
    </div>
    <div id="story-tree">${navigation}</div>
    <p class="empty" role="status">No frames match this filter.</p>
  </nav>
  <main class="viewer" id="viewer">${sections}</main>
</div>
<script>
(() => {
  const catalog = document.querySelector('.catalog');
  const search = document.querySelector('#story-search');
  const links = [...document.querySelectorAll('.frame-link')];
  const panels = [...document.querySelectorAll('[data-frame-panel]')];
  const filters = [...document.querySelectorAll('.filter')];
  let activeFilter = 'all';

  function selectFrame(index, focus = false) {
    links.forEach((link) => {
      const active = link.dataset.frame === String(index);
      link.classList.toggle('active', active);
      if (active) link.setAttribute('aria-current', 'page');
      else link.removeAttribute('aria-current');
    });
    panels.forEach((panel) => {
      panel.hidden = panel.dataset.framePanel !== String(index);
    });
    document.querySelector('#viewer').scrollTo({ top: 0 });
    if (focus) links.find((link) => link.dataset.frame === String(index))?.focus();
  }

  function applyFilters() {
    const query = search.value.trim().toLowerCase();
    let visibleCount = 0;
    links.forEach((link) => {
      const state = link.dataset.state;
      const stateMatches =
        activeFilter === 'all' ||
        (activeFilter === 'attention' && ['failed', 'changed', 'new'].includes(state)) ||
        (activeFilter === 'passed' && ['passed', 'matched'].includes(state));
      const visible = stateMatches && link.dataset.search.includes(query);
      link.closest('li').hidden = !visible;
      if (visible) visibleCount += 1;
    });
    document.querySelectorAll('.story-group').forEach((group) => {
      const visible = Boolean(group.querySelector('li:not([hidden])'));
      group.hidden = !visible;
      if (query && visible) group.open = true;
    });
    catalog.classList.toggle('no-results', visibleCount === 0);
    const selected = links.find((link) => link.classList.contains('active'));
    const next =
      selected && !selected.closest('li').hidden
        ? selected
        : links.find((link) => !link.closest('li').hidden);
    if (next) selectFrame(Number(next.dataset.frame));
    else panels.forEach((panel) => (panel.hidden = true));
  }

  links.forEach((link) => {
    link.addEventListener('click', () => selectFrame(Number(link.dataset.frame)));
    link.addEventListener('keydown', (event) => {
      if (!['ArrowDown', 'ArrowUp'].includes(event.key)) return;
      event.preventDefault();
      const visible = links.filter((candidate) => !candidate.closest('li').hidden);
      const current = visible.indexOf(link);
      const delta = event.key === 'ArrowDown' ? 1 : -1;
      const next = visible[(current + delta + visible.length) % visible.length];
      if (next) selectFrame(Number(next.dataset.frame), true);
    });
  });
  search.addEventListener('input', applyFilters);
  filters.forEach((filter) => {
    filter.addEventListener('click', () => {
      activeFilter = filter.dataset.filter;
      filters.forEach((candidate) => candidate.classList.toggle('active', candidate === filter));
      applyFilters();
    });
  });
})();
</script>
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

function formatCoverage(value: number): string {
  return `${value.toFixed(1)}%`;
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
    const terminalFrame = parseTerminalFrame(frame.frame);
    fs.writeFileSync(
      path.join(outputDirectory, `${filename}.html`),
      standaloneFrameDocument(renderTerminalFrameHtml(terminalFrame))
    );
    fs.writeFileSync(
      path.join(outputDirectory, `${filename}.txt`),
      terminalFrameText(terminalFrame).join('\n')
    );
    fs.writeFileSync(
      path.join(outputDirectory, `${filename}.frame.json`),
      JSON.stringify(frame.frame, null, 2)
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
  fs.writeFileSync(
    path.join(outputDirectory, 'coverage.json'),
    JSON.stringify(manifest.coverage, null, 2)
  );
  fs.writeFileSync(
    path.join(outputDirectory, 'coverage.md'),
    visualCoverageMarkdown(manifest.coverage)
  );
  fs.writeFileSync(
    path.join(outputDirectory, 'coverage-summary.md'),
    visualCoverageSummaryMarkdown(manifest.coverage)
  );
  fs.writeFileSync(
    path.join(outputDirectory, 'coverage.html'),
    visualCoverageHtml(manifest.coverage)
  );
}

async function captureVariant(
  storyId: string,
  storyName: string,
  variant: StorybookVariant,
  frames: CapturedFrame[]
): Promise<void> {
  const certification = variant.parameters.certification;
  const viewport = certification?.viewport ?? DEFAULT_VIEWPORT;
  const settleMs = certification?.settleMs ?? DEFAULT_SETTLE_MS;
  const tuiRoot = path.resolve(import.meta.dir, '../..');
  const runner = path.join(import.meta.dir, 'run-storybook.tsx');
  const kiroHome = fs.mkdtempSync(
    path.join(os.tmpdir(), 'kiro-storybook-visual-')
  );
  const pty = new PtyManager({
    width: viewport.columns,
    height: viewport.rows,
    cwd: tuiRoot,
    env: {
      ...certification?.environment,
      CI: 'true',
      FORCE_COLOR: '3',
      KIRO_AGENT_ENGINE: certification?.environment?.KIRO_AGENT_ENGINE ?? 'v2',
      KIRO_HOME: kiroHome,
      KIRO_STORYBOOK_PAUSE_ANIMATIONS: '1',
      KIRO_STORYBOOK_STORY: storyId,
      KIRO_STORYBOOK_VARIANT: variant.id,
    },
  });
  const capturedIds = new Set<string>();
  let scenarioError: string | undefined;

  const recordCapture = async (
    captureId: string,
    label: string,
    assertions?: StorybookAssertions
  ): Promise<void> => {
    validateCaptureId(captureId);
    if (capturedIds.has(captureId)) {
      throw new Error(`Duplicate visual capture id "${captureId}"`);
    }
    const frame = await pty.captureVisibleFrame({
      quietMs: settleMs,
      timeoutMs: 10000,
    });
    const text = terminalFrameText(frame);
    const failures = [
      ...rendererFailureMessages(text),
      ...assertionFailures(
        text,
        mergeStoryAssertions(certification?.assertions, assertions)
      ),
    ];
    const exitCode = pty.getExitCode();
    const processError =
      exitCode === undefined
        ? undefined
        : `Story process exited with code ${exitCode}`;
    const error = [scenarioError, processError, ...failures]
      .filter((message): message is string => message !== undefined)
      .join('; ');
    capturedIds.add(captureId);
    frames.push({
      key: `${storyId}--${variant.id}--${captureId}`,
      storyId,
      storyName,
      variantId: variant.id,
      variantName: variant.name,
      captureId,
      captureLabel: label,
      label: `${storyName} / ${label}`,
      frame: serializeTerminalFrame(frame),
      status: error ? 'failed' : 'passed',
      ...(error ? { error } : {}),
    });
  };

  const capture = async (requestedId: string): Promise<void> => {
    const resolved = resolveCaptureDefinition(
      requestedId,
      certification?.captures
    );
    await recordCapture(
      resolved.id,
      resolved.definition.label,
      resolved.definition.assertions
    );
  };

  const recordFailure = async (error: string): Promise<void> => {
    const captureId = capturedIds.has('failure')
      ? 'failure-diagnostic'
      : 'failure';
    let frame: SerializedTerminalFrame;
    let captureError: string | undefined;
    try {
      frame = serializeTerminalFrame(
        await pty.captureVisibleFrame({
          quietMs: settleMs,
          timeoutMs: 10000,
        })
      );
    } catch (failure) {
      captureError =
        failure instanceof Error ? failure.message : String(failure);
      let lines: readonly string[];
      try {
        lines = pty.getVisibleSnapshot();
      } catch {
        lines = [];
      }
      frame = serializeDiagnosticTextFrame(viewport, lines);
    }
    capturedIds.add(captureId);
    frames.push({
      key: `${storyId}--${variant.id}--${captureId}`,
      storyId,
      storyName,
      variantId: variant.id,
      variantName: variant.name,
      captureId,
      captureLabel: `${variant.name} - capture failure`,
      label: `${storyName} / ${variant.name} - capture failure`,
      frame,
      status: 'failed',
      error: [error, captureError && `Frame capture failed: ${captureError}`]
        .filter((message): message is string => Boolean(message))
        .join('; '),
    });
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
    if (certification) {
      await pty.waitForVisibleText(certification.readyText, 10000);
    } else {
      await waitForRenderedFrame(pty);
    }
    await sleep(settleMs);
    await variant.play?.(playContext);
    if (certification?.captures) {
      const missing = missingCaptureIds(certification.captures, capturedIds);
      if (missing.length > 0) {
        throw new Error(
          `Visual journey missed declared captures: ${missing.join(', ')}`
        );
      }
    } else if (capturedIds.size === 0) {
      await recordCapture('default', variant.name);
    }
  } catch (error) {
    scenarioError = error instanceof Error ? error.message : String(error);
    await recordFailure(scenarioError);
  } finally {
    try {
      pty.kill();
      await pty.expectExit(5000);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.warn(`Storybook PTY teardown failed: ${message}`);
    } finally {
      pty.dispose();
    }
    try {
      fs.rmSync(kiroHome, {
        force: true,
        maxRetries: 5,
        recursive: true,
        retryDelay: 100,
      });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.warn(`Storybook KIRO_HOME cleanup failed: ${message}`);
    }
  }
}

function recordVariantHarnessFailure(
  storyId: string,
  storyName: string,
  variant: StorybookVariant,
  frames: CapturedFrame[],
  error: unknown
): void {
  const baseCaptureId = 'harness-failure';
  let captureId = baseCaptureId;
  let suffix = 2;
  while (
    frames.some(
      (frame) =>
        frame.storyId === storyId &&
        frame.variantId === variant.id &&
        frame.captureId === captureId
    )
  ) {
    captureId = `${baseCaptureId}-${suffix}`;
    suffix += 1;
  }
  const message = error instanceof Error ? error.message : String(error);
  const viewport =
    variant.parameters.certification?.viewport ?? DEFAULT_VIEWPORT;
  frames.push({
    key: `${storyId}--${variant.id}--${captureId}`,
    storyId,
    storyName,
    variantId: variant.id,
    variantName: variant.name,
    captureId,
    captureLabel: `${variant.name} - harness failure`,
    label: `${storyName} / ${variant.name} - harness failure`,
    frame: serializeDiagnosticTextFrame(viewport, [message]),
    status: 'failed',
    error: `Visual story harness failed: ${message}`,
  });
}

async function main(): Promise<void> {
  const suite = option('suite') ?? DEFAULT_SUITE;
  const storyId = option('story');
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
    story.id !== storyId && storyId !== undefined
      ? []
      : story.variants
          .filter((variant) => isVariantSelectedForVisualSuite(variant, suite))
          .map((variant) => ({ story, variant }))
  );

  if (variants.length === 0) {
    const story = storyId ? ` and story "${storyId}"` : '';
    throw new Error(`No Visual Stories variants found for "${suite}"${story}`);
  }

  const plannedCoverage = collectVisualCoverage(
    path.resolve(import.meta.dir, '..'),
    stories,
    'planned',
    suite,
    storyId
  );

  for (const { story, variant } of variants) {
    process.stdout.write(`Capturing ${story.name} / ${variant.name}... `);
    const firstFrame = frames.length;
    try {
      await captureVariant(story.id, story.name, variant, frames);
    } catch (error) {
      recordVariantHarnessFailure(story.id, story.name, variant, frames, error);
    }
    const failed = frames
      .slice(firstFrame)
      .some((frame) => frame.status === 'failed');
    process.stdout.write(`${failed ? 'failed' : 'passed'}\n`);
  }

  const successfulVariantIds = new Set(
    variants.flatMap(({ story, variant }) => {
      const variantFrames = frames.filter(
        (frame) => frame.storyId === story.id && frame.variantId === variant.id
      );
      return variantFrames.length > 0 &&
        variantFrames.every((frame) => frame.status === 'passed')
        ? [`${story.id}/${variant.id}`]
        : [];
    })
  );
  const successfulCaptureIds = new Set(
    frames.flatMap((frame) =>
      frame.status === 'passed'
        ? [`${frame.storyId}/${frame.variantId}#${frame.captureId}`]
        : []
    )
  );
  const finalized = finalizeVisualEvidence(
    plannedCoverage,
    () =>
      collectVisualCoverage(
        path.resolve(import.meta.dir, '..'),
        stories,
        { successfulCaptureIds, successfulVariantIds },
        suite,
        storyId
      ),
    ({ coverage, collectionError }) => {
      const manifest: CaptureManifest = {
        version: 2,
        suite,
        generatedAt: new Date().toISOString(),
        coverage,
        frames,
        ...(collectionError
          ? {
              evidenceErrors: [
                `Visual coverage collection failed: ${collectionError}`,
              ],
            }
          : {}),
      };
      writeEvidence(outputDirectory, manifest, baseline);
    }
  );
  const failures = frames.filter((frame) => frame.status === 'failed');
  console.log(`Report: ${path.join(outputDirectory, 'index.html')}`);
  console.log(`Coverage: ${path.join(outputDirectory, 'coverage.html')}`);
  const errors = [
    ...(finalized.collectionError
      ? [`Visual coverage collection failed: ${finalized.collectionError}`]
      : []),
    ...(failures.length > 0
      ? [`${failures.length} visual story frame(s) failed`]
      : []),
  ];
  if (errors.length > 0) {
    throw new Error(errors.join('; '));
  }
}

await main();
