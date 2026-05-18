import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import {
  loadArtifactSummary,
  resolveArtifactPath,
  ARTIFACT_MAX_BYTES,
} from '../spec-artifact-loader';

describe('spec-artifact-loader', () => {
  let workspace: string;

  beforeEach(() => {
    workspace = mkdtempSync(join(tmpdir(), 'artifact-loader-test-'));
  });

  afterEach(() => {
    rmSync(workspace, { recursive: true, force: true });
  });

  function makeSpec(featureName: string, files: Record<string, string>): void {
    const dir = join(workspace, '.kiro', 'specs', featureName);
    mkdirSync(dir, { recursive: true });
    for (const [name, content] of Object.entries(files)) {
      writeFileSync(join(dir, name), content);
    }
  }

  describe('resolveArtifactPath', () => {
    it('joins workspace + .kiro/specs + feature + artifact', () => {
      const p = resolveArtifactPath('/work', 'login', 'requirements');
      expect(p).toBe('/work/.kiro/specs/login/requirements.md');
    });
  });

  describe('loadArtifactSummary', () => {
    it('returns FeatureNotFound when feature directory is missing', async () => {
      const result = await loadArtifactSummary(
        workspace,
        'nope',
        'requirements'
      );
      expect(result.ok).toBe(false);
      if (result.ok) throw new Error('expected error');
      expect(result.error.kind).toBe('FeatureNotFound');
    });

    it('returns ArtifactNotFound when feature exists but artifact does not', async () => {
      makeSpec('partial', { 'requirements.md': '# r' });
      const result = await loadArtifactSummary(workspace, 'partial', 'tasks');
      expect(result.ok).toBe(false);
      if (result.ok) throw new Error('expected error');
      expect(result.error.kind).toBe('ArtifactNotFound');
    });

    it('parses requirements successfully', async () => {
      makeSpec('login', {
        'requirements.md': [
          '### Requirement 1: A',
          '**User Story:** us-a',
          '### Requirement 2: B',
          '**User Story:** us-b',
        ].join('\n'),
      });
      const result = await loadArtifactSummary(
        workspace,
        'login',
        'requirements'
      );
      expect(result.ok).toBe(true);
      if (!result.ok) throw new Error('unexpected error');
      expect(result.summary.kind).toBe('requirements');
      if (result.summary.kind !== 'requirements') throw new Error();
      expect(result.summary.items).toHaveLength(2);
    });

    it('parses tasks successfully', async () => {
      makeSpec('build', {
        'tasks.md': [
          '- [ ] 1. Build it',
          '  - [ ] 1.1. Sub',
          '- [x] 2. Done',
        ].join('\n'),
      });
      const result = await loadArtifactSummary(workspace, 'build', 'tasks');
      expect(result.ok).toBe(true);
      if (!result.ok) throw new Error();
      if (result.summary.kind !== 'tasks') throw new Error();
      expect(result.summary.items).toHaveLength(2);
      expect(result.summary.items[0]!.subTasks).toHaveLength(1);
    });

    it('parses design successfully', async () => {
      makeSpec('design-feat', {
        'design.md': [
          '# Design',
          '## Introduction',
          'Intro paragraph.',
          '## Architecture',
          'Arch body.',
        ].join('\n'),
      });
      const result = await loadArtifactSummary(
        workspace,
        'design-feat',
        'design'
      );
      expect(result.ok).toBe(true);
      if (!result.ok) throw new Error();
      if (result.summary.kind !== 'design') throw new Error();
      expect(result.summary.overview).toBe('Intro paragraph.');
      expect(result.summary.sections).toHaveLength(2);
    });

    it('handles empty file content', async () => {
      makeSpec('empty', { 'requirements.md': '' });
      const result = await loadArtifactSummary(
        workspace,
        'empty',
        'requirements'
      );
      expect(result.ok).toBe(true);
      if (!result.ok) throw new Error();
      if (result.summary.kind !== 'requirements') throw new Error();
      expect(result.summary.items).toEqual([]);
    });

    it('rejects files larger than ARTIFACT_MAX_BYTES', async () => {
      const big = 'x'.repeat(ARTIFACT_MAX_BYTES + 1);
      makeSpec('big', { 'requirements.md': big });
      const result = await loadArtifactSummary(
        workspace,
        'big',
        'requirements'
      );
      expect(result.ok).toBe(false);
      if (result.ok) throw new Error();
      expect(result.error.kind).toBe('TooLarge');
      if (result.error.kind === 'TooLarge') {
        expect(result.error.sizeBytes).toBeGreaterThan(ARTIFACT_MAX_BYTES);
      }
    });
  });
});
