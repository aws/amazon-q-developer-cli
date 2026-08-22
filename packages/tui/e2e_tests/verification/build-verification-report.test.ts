import { describe, expect, it, spyOn } from 'bun:test';
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
        version: 2,
        suite: 'workflow-monitor',
        generatedAt: '2026-08-11T00:00:00.000Z',
        frames: [{ status: 'passed' }, { status: 'failed' }],
        coverage: {
          totalStories: 45,
          coveredStories: 5,
          totalVariants: 215,
          executedVariants: 28,
          verifiedVariants: 21,
          totalVisualStates: 69,
          coveredVisualStates: 58,
          totalComponents: 206,
          coveredComponents: 74,
          stories: [{ status: 'ignored by aggregate report' }],
        },
      })
    );
    fs.writeFileSync(path.join(visualDir, 'index.html'), '<html></html>');

    const collected = collectVerificationArtifacts(root);
    expect(collected.scenarioReports).toHaveLength(1);
    expect(collected.visualSuites).toHaveLength(1);
    expect(collected.visualSuites[0]?.reportPath).toBe(
      'verification-visual-workflow-monitor/index.html'
    );
    expect(collected.visualSuites[0]?.coverage).toEqual({
      totalStories: 45,
      coveredStories: 5,
      totalVariants: 215,
      executedVariants: 28,
      verifiedVariants: 21,
      totalVisualStates: 69,
      coveredVisualStates: 58,
      totalComponents: 206,
      coveredComponents: 74,
    });

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
    expect(markdown).toContain(
      '| 5/45 (11.1%) | 28/215 (13.0%) | 21/215 (9.8%) | 58/69 (84.1%) | 74/206 (35.9%) |'
    );
    expect(markdown).toContain('Semantic variants have explicit assertions');
    expect(markdown).not.toContain('Interaction');
    expect(markdown).not.toContain('[json](');
    expect(markdown).not.toContain('[artifact](');

    const html = fs.readFileSync(path.join(outDir, 'index.html'), 'utf8');
    expect(html).toContain(
      'href="artifacts/verification-smoke/scenario-report-acp-mock-123.json"'
    );
    expect(html).toContain(
      'href="artifacts/verification-visual-workflow-monitor/index.html"'
    );
    expect(html).toContain('<th>Component Coverage</th>');
    expect(html).toContain('<td>74/206 (35.9%)</td>');
    expect(html).toContain('<th>Semantic Variants</th>');
    expect(html).toContain('<td>21/215 (9.8%)</td>');
    expect(html).toContain('<td>58/69 (84.1%)</td>');
    expect(html).toContain('Semantic variants have explicit assertions');
    expect(html).not.toContain('Interaction');
  });

  it('reports unavailable coverage for legacy visual manifests', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-verification-'));
    const visualDir = path.join(root, 'verification-visual-legacy');
    const outDir = path.join(root, 'summary');

    fs.mkdirSync(visualDir, { recursive: true });
    fs.writeFileSync(
      path.join(visualDir, 'manifest.json'),
      JSON.stringify({
        version: 1,
        suite: 'legacy',
        generatedAt: '2026-08-11T00:00:00.000Z',
        frames: [{ status: 'passed' }],
      })
    );

    writeVerificationReport(root, outDir);
    const markdown = fs.readFileSync(path.join(outDir, 'summary.md'), 'utf8');
    expect(markdown).toContain(
      '| verification-visual-legacy | legacy | 1 | 0 | n/a | n/a | n/a | n/a | n/a |'
    );
  });

  it('warns and skips unsupported visual manifest versions', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-verification-'));
    const visualDir = path.join(root, 'verification-visual-unsupported');
    const warn = spyOn(console, 'warn').mockImplementation(() => {});

    try {
      fs.mkdirSync(visualDir, { recursive: true });
      fs.writeFileSync(
        path.join(visualDir, 'manifest.json'),
        JSON.stringify({
          version: 3,
          suite: 'unsupported',
          generatedAt: '2026-08-11T00:00:00.000Z',
          frames: [{ status: 'passed' }],
        })
      );

      expect(collectVerificationArtifacts(root).visualSuites).toEqual([]);
      expect(warn).toHaveBeenCalledWith(
        'Skipping unsupported visual manifest version at verification-visual-unsupported/manifest.json: 3'
      );
    } finally {
      warn.mockRestore();
    }
  });

  it('rejects inconsistent visual coverage values', () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-verification-'));
    const visualDir = path.join(root, 'verification-visual-invalid');

    fs.mkdirSync(visualDir, { recursive: true });
    fs.writeFileSync(
      path.join(visualDir, 'manifest.json'),
      JSON.stringify({
        version: 1,
        suite: 'invalid',
        generatedAt: '2026-08-11T00:00:00.000Z',
        frames: [{ status: 'passed' }],
        coverage: {
          totalStories: 1,
          coveredStories: 2,
          totalVariants: 1,
          executedVariants: 1,
          verifiedVariants: 1,
          totalVisualStates: 1,
          coveredVisualStates: 1,
          totalComponents: 1,
          coveredComponents: 1,
        },
      })
    );

    const collected = collectVerificationArtifacts(root);
    expect(collected.visualSuites[0]?.coverage).toBeUndefined();
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
