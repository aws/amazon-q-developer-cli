/**
 * LiteSequence — optional sugar over E2ETestCase for the Knight Rider
 * compositional scenarios (S1-S5 in docs/design/lite-tui-action-items.md).
 *
 * The scenarios are deliberately long sequences of cross-component
 * interactions; readability + post-mortem evidence matter more than they
 * do in focused tests. This helper:
 *
 *   1. Wraps `step(label, fn)` so the call site reads as a timeline.
 *   2. Wraps `expect(label, predicate)` so the failed-step name lands in
 *      the error message (bun-test's stack trace alone doesn't tell you
 *      WHICH of 12 sequential expects blew up).
 *   3. Captures per-step HTML snapshots in memory; `dumpHtml()` writes
 *      them out alongside the test's regular snapshot.html on failure.
 *      Default is dump-on-failure (caller's try/finally) so the
 *      test-outputs/ tree doesn't fill with KB of evidence for every
 *      successful run.
 *
 * No additions to E2ETestCase. No subclassing. Tests that don't want the
 * timeline can keep using E2ETestCase directly.
 */

import * as fs from 'fs';
import * as path from 'path';
import type { AppState } from '../../../src/stores/app-store';
import type { E2ETestCase } from '../../E2ETestCase';

interface SequenceStep {
  label: string;
  kind: 'step' | 'expect';
  ts: number;
  html: string;
  store?: Pick<
    AppState,
    | 'isProcessing'
    | 'queuedMessages'
    | 'uiMode'
    | 'pendingApproval'
    | 'subagentPanelOpen'
    | 'sessionId'
  >;
  ok: boolean;
  error?: string;
}

export class LiteSequence {
  private steps: SequenceStep[] = [];
  private startedAt: number;

  constructor(
    private tc: E2ETestCase,
    private name: string
  ) {
    this.startedAt = Date.now();
  }

  /**
   * Run a step (an action that drives the TUI). Captures the post-step
   * HTML snapshot for the on-failure dump. Re-throws on failure so the
   * test fails loudly; the partial timeline is still preserved.
   */
  async step(label: string, fn: () => Promise<void>): Promise<void> {
    let ok = true;
    let errMsg: string | undefined;
    try {
      await fn();
    } catch (e) {
      ok = false;
      errMsg = e instanceof Error ? e.message : String(e);
      this.recordStep('step', label, ok, errMsg);
      throw e;
    }
    this.recordStep('step', label, ok);
  }

  /**
   * Assert an invariant against the current store. The label is
   * incorporated into the error so a failure says "expect(<label>) failed"
   * rather than just "AssertionError: expected true to be true".
   */
  async expect(
    label: string,
    predicate: (s: AppState) => boolean
  ): Promise<void> {
    const store = await this.tc.getStore();
    const ok = predicate(store);
    this.recordStep('expect', label, ok);
    if (!ok) {
      throw new Error(`LiteSequence.expect("${label}") failed`);
    }
  }

  private recordStep(
    kind: 'step' | 'expect',
    label: string,
    ok: boolean,
    error?: string
  ): void {
    let html = '';
    let store: SequenceStep['store'];
    try {
      html = this.tc.getSnapshotHtml();
    } catch {
      // ignore snapshot errors during a torn-down test
    }
    // Best-effort store sample; never block the timeline if IPC is gone.
    void this.tc
      .getStore()
      .then((s) => {
        store = {
          isProcessing: s.isProcessing,
          queuedMessages: s.queuedMessages,
          uiMode: s.uiMode,
          pendingApproval: s.pendingApproval,
          subagentPanelOpen: s.subagentPanelOpen,
          sessionId: s.sessionId,
        };
        const last = this.steps[this.steps.length - 1];
        if (last && last.label === label) last.store = store;
      })
      .catch(() => {
        /* ignore */
      });
    this.steps.push({
      label,
      kind,
      ts: Date.now() - this.startedAt,
      html,
      store,
      ok,
      error,
    });
  }

  /**
   * Write a per-step HTML timeline alongside the test's regular outputs.
   * Call from a try/finally on failure so successful runs don't bloat
   * test-outputs/. The output is one HTML file per step, plus an
   * `index.html` that links them in order.
   */
  async dumpHtml(): Promise<string | null> {
    if (this.steps.length === 0) return null;
    const outDir = path.join(
      this.tc.sandboxDir,
      '..',
      `${this.name}-timeline-${this.startedAt}`
    );
    try {
      fs.mkdirSync(outDir, { recursive: true });
    } catch {
      return null;
    }
    const indexLines: string[] = [
      '<!doctype html><html><head><meta charset="utf-8">',
      `<title>LiteSequence — ${this.name}</title>`,
      '<style>body{font-family:system-ui;background:#111;color:#ddd;padding:24px}',
      'a{color:#9cf;display:block;margin:6px 0;font-family:monospace}',
      '.bad{color:#f88}',
      '</style></head><body>',
      `<h1>${this.name}</h1>`,
      `<p>${this.steps.length} timeline entries.</p>`,
    ];
    for (let i = 0; i < this.steps.length; i++) {
      const s = this.steps[i]!;
      const fname = `${String(i).padStart(3, '0')}-${s.kind}-${s.label.replace(/[^A-Za-z0-9_-]+/g, '_').slice(0, 50)}.html`;
      try {
        fs.writeFileSync(path.join(outDir, fname), s.html);
      } catch {
        /* ignore individual step write failures */
      }
      const cls = s.ok ? '' : ' class="bad"';
      indexLines.push(
        `<a href="${fname}"${cls}>+${s.ts}ms · ${s.kind} · ${s.label}${s.ok ? '' : ' [FAILED' + (s.error ? `: ${s.error}` : '') + ']'}</a>`
      );
    }
    indexLines.push('</body></html>');
    try {
      fs.writeFileSync(path.join(outDir, 'index.html'), indexLines.join('\n'));
    } catch {
      /* ignore */
    }
    return outDir;
  }

  /** Number of timeline entries recorded so far. Used in tests for sanity checks. */
  get length(): number {
    return this.steps.length;
  }
}
