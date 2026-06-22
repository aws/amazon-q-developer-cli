/** Timeline helper for KR lite scenarios; dumps per-step HTML on failure. */

import * as fs from 'fs';
import * as path from 'path';
import type { AppState } from '../../../src/stores/app-store';
import type { E2ETestCase } from '../../E2ETestCase';

interface SequenceStep {
  label: string;
  kind: 'step' | 'expect';
  ts: number;
  html: string;
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

  /** Label is folded into the thrown error so the failing step is named. */
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
    try {
      html = this.tc.getSnapshotHtml();
    } catch {
      // ignore snapshot errors during a torn-down test
    }
    this.steps.push({
      label,
      kind,
      ts: Date.now() - this.startedAt,
      html,
      ok,
      error,
    });
  }

  /** Write a per-step HTML timeline; call on failure so passing runs don't bloat test-outputs/. */
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

  get length(): number {
    return this.steps.length;
  }
}
