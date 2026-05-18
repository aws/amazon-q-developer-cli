#!/usr/bin/env bun
/**
 * markdown-rendering.ts — Knight Rider probe: markdown rendering fidelity.
 *
 * Sends a prompt that elicits a response containing all markdown constructs,
 * then captures frames and checks for raw/unrendered markers in the output.
 *
 * Guards against the recurring class of bugs where specific markdown elements
 * render as raw text (7+ fixes in Apr 2026):
 *   - * and + list markers rendered literally
 *   - Blockquote inline markdown not parsed
 *   - Tables overflowing terminal width
 *   - Blank lines swallowed at exact-width boundaries
 *   - Horizontal rules (*** / ___) rendered as text
 *
 * Exit codes: 0 = pass, 1 = finding, 2 = probe crash
 */

import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const PROBE_NAME = 'markdown-rendering';
const PLATFORM = process.env.KIRO_PROBE_PLATFORM ?? (process.platform === 'darwin' ? 'macos' : process.platform);
const OUTPUT_DIR = process.env.PROBE_OUTPUT_DIR ?? './probe-output';
const KR_URL = process.env.KNIGHT_RIDER_URL ?? 'http://localhost:3001';
const KR = `${KR_URL}/api`;

// Timeout for waiting for the agent response
const RESPONSE_TIMEOUT_MS = 90_000;

function timestamp(): string {
  const now = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${now.getUTCFullYear()}${pad(now.getUTCMonth() + 1)}${pad(now.getUTCDate())}-${pad(now.getUTCHours())}${pad(now.getUTCMinutes())}`;
}

function slugify(text: string): string {
  return text.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 50);
}

const TS = timestamp();
const PREFIX = `${PROBE_NAME}-${PLATFORM}-${TS}`;

function writeFinding(opts: {
  slug: string;
  title: string;
  severity: 'crash' | 'spiral' | 'regression' | 'slowdown' | 'smell';
  file: string;
  description: string;
  evidence: string;
  proposedFix?: string;
}) {
  const slug = slugify(opts.slug);
  const findingId = `${PREFIX}-${slug}`;
  const filePath = join(OUTPUT_DIR, `${findingId}.md`);
  writeFileSync(filePath, [
    '---',
    `id: ${findingId}`,
    `work-item: ${PROBE_NAME}`,
    `review: 01-async-render-path`,
    `technique: 3`,
    `class: markdown-rendering`,
    `severity: ${opts.severity}`,
    `file: ${opts.file}`,
    `platforms-affected: [${PLATFORM}]`,
    `discovered-by: blackbox`,
    `discovered-at: ${new Date().toISOString()}`,
    `status: open`,
    '---',
    '',
    `# ${opts.title}`,
    '',
    opts.description,
    '',
    '## Evidence',
    '',
    opts.evidence,
    opts.proposedFix ? `\n## Proposed fix\n\n${opts.proposedFix}` : '',
    '',
  ].join('\n'));
  return filePath;
}

function writeDoneMarker(findingsEmitted: number, elapsedMs: number) {
  writeFileSync(join(OUTPUT_DIR, `${PREFIX}-done.md`), [
    '---',
    `id: ${PREFIX}-done`,
    `work-item: ${PROBE_NAME}`,
    `kind: blackbox`,
    `platform: ${PLATFORM}`,
    `status: done`,
    `findings-emitted: ${findingsEmitted}`,
    `elapsed-ms: ${elapsedMs}`,
    `completed-at: ${new Date().toISOString()}`,
    '---',
    '',
    `# Probe done: ${PROBE_NAME} on ${PLATFORM}`,
    '',
    `Emitted ${findingsEmitted} finding(s) in ${elapsedMs} ms.`,
    '',
  ].join('\n'));
}

function writeMetrics(metrics: Record<string, unknown>) {
  writeFileSync(join(OUTPUT_DIR, `${PREFIX}-metrics.json`), JSON.stringify(metrics, null, 2));
}

async function kr(method: string, path: string, body?: unknown): Promise<any> {
  const res = await fetch(`${KR}${path}`, {
    method,
    headers: body ? { 'Content-Type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) throw new Error(`KR ${method} ${path}: ${res.status} ${await res.text()}`);
  return res.json();
}

async function frame(label: string) { return kr('POST', '/frame', { label }); }
async function screen(): Promise<string[]> { return (await kr('GET', '/screen')).lines; }
async function sleep(ms: number) { return kr('POST', '/sleep', { ms }); }
async function status(): Promise<{ ready: boolean }> { return kr('GET', '/status'); }

async function typeText(text: string) {
  for (const c of text) {
    await kr('POST', '/keys', { keys: c });
    await new Promise(r => setTimeout(r, 40));
  }
  await new Promise(r => setTimeout(r, 300));
}

async function waitForIdle(timeoutMs = RESPONSE_TIMEOUT_MS): Promise<boolean> {
  // Wait for the prompt to reappear (agent finished responding)
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    const lines = await screen();
    const text = lines.join('\n').toLowerCase();
    if (text.includes('ask a question') || text.includes('message kiro')) {
      return true;
    }
    await new Promise(r => setTimeout(r, 3000));
  }
  return false;
}

// ── Markdown checks ─────────────────────────────────────────────

interface Check {
  name: string;
  /** Raw markers that should NOT appear if rendering is correct */
  rawMarkers: RegExp[];
  /** Description of what we're looking for */
  description: string;
  file: string;
}

const CHECKS: Check[] = [
  {
    name: 'unrendered-bold',
    rawMarkers: [/\*\*[^*]+\*\*/],
    description: 'Bold markers (**text**) rendered as literal asterisks instead of styled text',
    file: 'packages/tui/src/components/chat/markdown/MarkdownRenderer.tsx',
  },
  {
    name: 'unrendered-inline-code',
    rawMarkers: [/(?<!`)`[^`\n]+`(?!`)/],
    description: 'Inline code backticks rendered literally instead of styled',
    file: 'packages/tui/src/components/chat/markdown/MarkdownRenderer.tsx',
  },
  {
    name: 'raw-list-markers',
    rawMarkers: [/^\s*[*+]\s+\S/m],
    description: 'List markers (* or +) rendered as raw text instead of bullet points',
    file: 'packages/tui/src/components/chat/markdown/MarkdownRenderer.tsx',
  },
  {
    name: 'raw-hr-markers',
    rawMarkers: [/^\s*(\*{3,}|_{3,})\s*$/m],
    description: 'Horizontal rules (*** or ___) rendered as raw text instead of a line',
    file: 'packages/tui/src/components/chat/markdown/MarkdownRenderer.tsx',
  },
  {
    name: 'raw-blockquote-markers',
    rawMarkers: [/^>\s.*\*\*[^*]+\*\*/m],
    description: 'Inline markdown inside blockquotes rendered as raw markers',
    file: 'packages/tui/src/components/chat/markdown/MarkdownRenderer.tsx',
  },
];

// The prompt asks the agent to produce a response with all markdown constructs.
// We use a system-prompt-style instruction that should work with any model.
const MARKDOWN_PROMPT = `Reply with EXACTLY this markdown (no changes, no explanation before or after):

# Heading 1

## Heading 2

This is **bold text** and this is \`inline code\`.

* First bullet item
* Second bullet item
+ Third with plus marker

1. Numbered one
2. Numbered two

> This is a blockquote with **bold** and \`code\` inside.

---

***

| Column A | Column B | Column C |
|----------|----------|----------|
| cell 1   | cell 2   | cell 3   |
| longer cell content here | short | medium length |

\`\`\`javascript
function hello() {
  return "world";
}
\`\`\`

___

End of test.`;

async function main() {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  const started = Date.now();
  let findingsEmitted = 0;

  // Check Knight Rider
  try {
    const s = await status();
    if (!s.ready) throw new Error('not ready');
  } catch {
    console.error(`[${PROBE_NAME}] Knight Rider not available at ${KR_URL}`);
    process.exit(2);
  }
  console.log(`[${PROBE_NAME}] Knight Rider ready`);

  // ── Send the markdown prompt ──────────────────────────────────
  console.log(`[${PROBE_NAME}] Sending markdown prompt...`);
  await frame('01-before-prompt');
  await typeText(MARKDOWN_PROMPT);
  await kr('POST', '/enter', undefined);

  // Wait for response
  console.log(`[${PROBE_NAME}] Waiting for response...`);
  const gotResponse = await waitForIdle();
  if (!gotResponse) {
    await frame('02-timeout');
    findingsEmitted++;
    writeFinding({
      slug: 'response-timeout',
      title: 'Agent did not respond within timeout',
      severity: 'crash',
      file: 'packages/tui/src/components/chat/ConversationView.tsx',
      description: `Agent did not finish responding within ${RESPONSE_TIMEOUT_MS / 1000}s. Cannot verify markdown rendering.`,
      evidence: 'See frame: 02-timeout',
    });
    const elapsed = Date.now() - started;
    writeMetrics({ probe: PROBE_NAME, platform: PLATFORM, findingsEmitted, elapsed, timedOut: true });
    writeDoneMarker(findingsEmitted, elapsed);
    process.exit(1);
  }

  await sleep(1000);
  await frame('02-full-response');

  // ── Capture all visible text ──────────────────────────────────
  const lines = await screen();
  const fullText = lines.join('\n');
  console.log(`[${PROBE_NAME}] Captured ${lines.length} lines, ${fullText.length} chars`);

  // ── Check for rendering failures ──────────────────────────────
  // Only check the assistant response area — skip the user prompt echo
  // which legitimately contains raw markdown markers.
  // Heuristic: find the separator line (────) between user message and response,
  // or fall back to text after "End of test." (our prompt's last line).

  // Also scroll up to see the full response if it's long
  for (let i = 0; i < 5; i++) {
    await kr('POST', '/up', undefined);
    await new Promise(r => setTimeout(r, 200));
  }
  await sleep(500);
  const scrolledLines = await screen();
  await frame('03-scrolled-up');
  const scrolledText = scrolledLines.join('\n');

  // Combine both views
  const allVisibleText = fullText + '\n' + scrolledText;

  // Extract only the assistant response area
  // Strategy: find "End of test." (last line of our prompt), then take everything after.
  // The TUI renders a separator (────) between user and assistant — use that as backup.
  let responseArea = allVisibleText;
  const endOfPromptIdx = allVisibleText.indexOf('End of test.');
  if (endOfPromptIdx > 0) {
    responseArea = allVisibleText.slice(endOfPromptIdx + 'End of test.'.length);
  }
  // Further narrow: if there's a horizontal rule separator (────), take text after it
  const separatorIdx = responseArea.indexOf('────');
  if (separatorIdx > 0) {
    responseArea = responseArea.slice(separatorIdx + 4);
  }

  const checkResults: { name: string; found: boolean; matches: string[] }[] = [];

  for (const check of CHECKS) {
    const matches: string[] = [];
    for (const marker of check.rawMarkers) {
      const m = responseArea.match(new RegExp(marker.source, 'gm'));
      if (m) matches.push(...m.slice(0, 3));
    }
    checkResults.push({ name: check.name, found: matches.length > 0, matches });
  }

  // ── Table overflow check ──────────────────────────────────────
  // Check if any line exceeds terminal width (table not wrapping)
  const termWidth = 120; // Knight Rider default
  const overflowLines = lines.filter(l => l.length > termWidth + 5); // small tolerance for ANSI
  if (overflowLines.length > 0) {
    checkResults.push({
      name: 'table-overflow',
      found: true,
      matches: overflowLines.slice(0, 3),
    });
  }

  // ── Blank content check ───────────────────────────────────────
  // After a response, there should be substantial visible content
  const nonEmptyLines = lines.filter(l => l.trim().length > 0);
  if (nonEmptyLines.length < 5) {
    findingsEmitted++;
    writeFinding({
      slug: 'response-not-visible',
      title: 'Response content not visible on screen',
      severity: 'regression',
      file: 'packages/tui/src/components/chat/ConversationView.tsx',
      description: `After agent responded, only ${nonEmptyLines.length} non-empty lines visible. Expected rendered markdown content.`,
      evidence: `Non-empty lines: ${nonEmptyLines.length}\nSee frames: 02-full-response, 03-scrolled-up`,
    });
  }

  // ── Emit findings for failed checks ───────────────────────────
  // Note: raw-list-markers and raw-hr-markers are the most reliable signals.
  // Bold/code checks may false-positive on the user prompt echo.
  // We only emit findings for the high-confidence checks.
  const highConfidence = ['raw-list-markers', 'raw-hr-markers', 'raw-blockquote-markers', 'table-overflow'];

  for (const result of checkResults) {
    if (!result.found) continue;
    if (!highConfidence.includes(result.name)) continue;

    const check = CHECKS.find(c => c.name === result.name);
    findingsEmitted++;
    writeFinding({
      slug: result.name,
      title: `Markdown rendering: ${result.name.replace(/-/g, ' ')}`,
      severity: 'regression',
      file: check?.file ?? 'packages/tui/src/components/chat/markdown/MarkdownRenderer.tsx',
      description: check?.description ?? `Raw markdown markers found in rendered output: ${result.name}`,
      evidence: `Matches found:\n\`\`\`\n${result.matches.join('\n')}\n\`\`\`\nSee frames: 02-full-response, 03-scrolled-up`,
      proposedFix: 'Check MarkdownRenderer line-by-line trigger conditions and inline parsing.',
    });
  }

  // ── Resize and re-check (width boundary bugs) ─────────────────
  console.log(`[${PROBE_NAME}] Phase 2: Resize to narrow width and check reflow`);
  await kr('POST', '/resize', { cols: 60, rows: 40 });
  await sleep(2000);
  await frame('04-narrow-reflow');

  const narrowLines = await screen();
  const narrowEmpty = narrowLines.filter(l => l.trim().length > 0).length;
  if (narrowEmpty < 3) {
    findingsEmitted++;
    writeFinding({
      slug: 'content-lost-on-narrow-reflow',
      title: 'Content disappeared after narrowing terminal',
      severity: 'regression',
      file: 'packages/twinki/packages/twinki/src/dom/static-output.ts',
      description: `After resizing to 60 cols, only ${narrowEmpty} non-empty lines visible. Content may have been lost during reflow.`,
      evidence: `Visible lines at 60 cols: ${narrowEmpty}\nSee frame: 04-narrow-reflow`,
    });
  }

  // Restore
  await kr('POST', '/resize', { cols: NORMAL_WIDTH, rows: 40 });
  await sleep(1000);
  await frame('05-restored');

  // ── Done ──────────────────────────────────────────────────────
  const elapsed = Date.now() - started;
  writeMetrics({
    probe: PROBE_NAME,
    platform: PLATFORM,
    findingsEmitted,
    elapsed,
    checks: checkResults.map(r => ({ name: r.name, found: r.found, matchCount: r.matches.length })),
  });
  writeDoneMarker(findingsEmitted, elapsed);

  const result = findingsEmitted > 0 ? 'FAIL' : 'PASS';
  console.log(`\n[${PROBE_NAME}] ${result} — ${findingsEmitted} finding(s), ${elapsed}ms`);
  console.log(`  Checks: ${checkResults.map(r => `${r.name}=${r.found ? 'FOUND' : 'ok'}`).join(', ')}`);
  process.exit(findingsEmitted > 0 ? 1 : 0);
}

const NORMAL_WIDTH = 120;

try {
  await main();
} catch (err) {
  mkdirSync(OUTPUT_DIR, { recursive: true });
  writeFileSync(join(OUTPUT_DIR, `${PREFIX}-error.log`), String(err instanceof Error ? (err.stack ?? err.message) : err));
  console.error(`[${PROBE_NAME}] probe crashed:`, err);
  process.exit(2);
}
