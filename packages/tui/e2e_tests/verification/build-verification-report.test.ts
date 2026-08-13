import { describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import {
  collectVerificationArtifacts,
  writeVerificationReport,
} from './build-verification-report';

describe('verification artifact report', () => {
  it('collects scenario and visual artifacts and writes a summary bundle', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-verification-'));
    const smokeDir = path.join(root, 'verification-smoke');
    const visualDir = path.join(root, 'verification-visual-workflow-monitor');
    const outDir = path.join(root, 'summary');

    fs.mkdirSync(smokeDir, { recursive: true });
    fs.mkdirSync(visualDir, { recursive: true });

    fs.writeFileSync(
      path.join(smokeDir, 'scenario-report-acp-mock-123.json'),
      JSON.stringify({
        backendId: 'acp-mock',
        engine: 'kas',
        total: 12,
        passed: 11,
        failed: 1,
        skipped: 0,
      })
    );

    fs.writeFileSync(
      path.join(visualDir, 'manifest.json'),
      JSON.stringify({
        version: 1,
        suite: 'workflow-monitor',
        generatedAt: '2026-08-11T00:00:00.000Z',
        frames: [{ status: 'passed' }, { status: 'failed' }],
      })
    );
    fs.writeFileSync(path.join(visualDir, 'index.html'), '<html></html>');

    const collected = collectVerificationArtifacts(root);
    expect(collected.scenarioReports).toHaveLength(1);
    expect(collected.visualSuites).toHaveLength(1);
    expect(collected.visualSuites[0]?.reportPath).toBe(
      'verification-visual-workflow-monitor/index.html'
    );

    writeVerificationReport(root, outDir);
    expect(fs.existsSync(path.join(outDir, 'summary.json'))).toBe(true);
    expect(fs.existsSync(path.join(outDir, 'summary.md'))).toBe(true);
    expect(fs.existsSync(path.join(outDir, 'index.html'))).toBe(true);
    expect(
      fs.existsSync(
        path.join(
          outDir,
          'artifacts',
          'verification-smoke',
          'scenario-report-acp-mock-123.json'
        )
      )
    ).toBe(true);
    expect(
      fs.existsSync(
        path.join(
          outDir,
          'artifacts',
          'verification-visual-workflow-monitor',
          'index.html'
        )
      )
    ).toBe(true);

    const markdown = fs.readFileSync(path.join(outDir, 'summary.md'), 'utf8');
    expect(markdown).toContain(
      '`artifacts/verification-smoke/scenario-report-acp-mock-123.json`'
    );
    expect(markdown).toContain(
      '`artifacts/verification-visual-workflow-monitor/index.html`'
    );
    expect(markdown).not.toContain('[json](');
    expect(markdown).not.toContain('[artifact](');

    const html = fs.readFileSync(path.join(outDir, 'index.html'), 'utf8');
    expect(html).toContain(
      'href="artifacts/verification-smoke/scenario-report-acp-mock-123.json"'
    );
    expect(html).toContain(
      'href="artifacts/verification-visual-workflow-monitor/index.html"'
    );
  });

  it('keeps only the newest scenario report per lane after retries', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-verification-'));
    const smokeDir = path.join(root, 'verification-smoke');

    fs.mkdirSync(smokeDir, { recursive: true });
    fs.writeFileSync(
      path.join(smokeDir, 'scenario-report-acp-mock-100.json'),
      JSON.stringify({
        backendId: 'acp-mock',
        engine: 'kas',
        total: 4,
        passed: 3,
        failed: 1,
        skipped: 0,
      })
    );
    fs.writeFileSync(
      path.join(smokeDir, 'scenario-report-acp-mock-200.json'),
      JSON.stringify({
        backendId: 'acp-mock',
        engine: 'kas',
        total: 1,
        passed: 1,
        failed: 0,
        skipped: 0,
      })
    );

    const collected = collectVerificationArtifacts(root);
    expect(collected.scenarioReports).toEqual([
      {
        artifact: 'verification-smoke',
        relativePath: 'verification-smoke/scenario-report-acp-mock-200.json',
        backendId: 'acp-mock',
        engine: 'kas',
        total: 1,
        passed: 1,
        failed: 0,
        skipped: 0,
      },
    ]);
  });

  it('returns an empty summary when the verification root does not exist', () => {
    const root = path.join(
      os.tmpdir(),
      `kiro-verification-missing-${Date.now()}`
    );
    const outDir = path.join(
      os.tmpdir(),
      `kiro-verification-summary-${Date.now()}`
    );

    const collected = collectVerificationArtifacts(root);
    expect(collected).toEqual({
      scenarioReports: [],
      visualSuites: [],
    });

    writeVerificationReport(root, outDir);
    expect(fs.existsSync(path.join(outDir, 'summary.json'))).toBe(true);
    expect(fs.existsSync(path.join(outDir, 'summary.md'))).toBe(true);
    expect(fs.existsSync(path.join(outDir, 'index.html'))).toBe(true);
  });
});
