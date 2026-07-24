import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync, symlinkSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

import {
  composeSpecKickoffPrompt,
  describeSpecDocuments,
  findSpecFeature,
  listSpecFeatures,
  specsRoot,
  type SpecFeatureSummary,
} from '../spec-workspace';

describe('composeSpecKickoffPrompt', () => {
  it('includes the feature name, directory, and description as ground truth', () => {
    const prompt = composeSpecKickoffPrompt(
      'slack bot',
      'Tracks design requests and helps prioritize them'
    );
    expect(prompt).toContain('Start a new spec called "slack bot"');
    expect(prompt).toContain('.kiro/specs/slack bot/');
    expect(prompt).toContain('ground truth');
    expect(prompt).toContain(
      'Tracks design requests and helps prioritize them'
    );
  });
});

describe('spec-workspace', () => {
  let workspaceRoot: string;

  beforeEach(() => {
    workspaceRoot = mkdtempSync(join(tmpdir(), 'spec-workspace-test-'));
  });

  afterEach(() => {
    rmSync(workspaceRoot, { recursive: true, force: true });
  });

  function makeSpec(featureName: string, files: string[]): string {
    const dir = join(workspaceRoot, '.kiro', 'specs', featureName);
    mkdirSync(dir, { recursive: true });
    for (const file of files) {
      writeFileSync(join(dir, file), 'content');
    }
    return dir;
  }

  describe('specsRoot()', () => {
    it('returns .kiro/specs under the workspace root', () => {
      expect(specsRoot('/work')).toBe('/work/.kiro/specs');
    });
  });

  describe('listSpecFeatures()', () => {
    it('returns empty array when .kiro/specs does not exist', () => {
      expect(listSpecFeatures(workspaceRoot)).toEqual([]);
    });

    it('returns empty array when .kiro/specs exists but is empty', () => {
      mkdirSync(join(workspaceRoot, '.kiro', 'specs'), { recursive: true });
      expect(listSpecFeatures(workspaceRoot)).toEqual([]);
    });

    it('discovers a feature with all spec documents', () => {
      makeSpec('login', [
        'requirements.md',
        'design.md',
        'tasks.md',
        'bugfix.md',
      ]);

      const features = listSpecFeatures(workspaceRoot);
      expect(features).toHaveLength(1);
      const f = features[0]!;
      expect(f.featureName).toBe('login');
      expect(f.documents).toEqual([
        'requirements.md',
        'design.md',
        'tasks.md',
        'bugfix.md',
      ]);
      expect(f.tasksFilePath).toBe(
        join(workspaceRoot, '.kiro', 'specs', 'login', 'tasks.md')
      );
      expect(f.specDocumentPaths).toHaveLength(4);
    });

    it('discovers a feature with only some spec documents', () => {
      makeSpec('partial', ['requirements.md', 'design.md']);

      const features = listSpecFeatures(workspaceRoot);
      expect(features).toHaveLength(1);
      const f = features[0]!;
      expect(f.documents).toEqual(['requirements.md', 'design.md']);
      expect(f.tasksFilePath).toBeUndefined();
      expect(f.specDocumentPaths).toHaveLength(2);
    });

    it('skips directories with no recognised spec documents', () => {
      // Directory exists but contains nothing the workflow recognises.
      const dir = join(workspaceRoot, '.kiro', 'specs', 'noise');
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, 'README.md'), 'hi');
      writeFileSync(join(dir, 'notes.txt'), 'hi');

      expect(listSpecFeatures(workspaceRoot)).toEqual([]);
    });

    it('skips dotfiles and dotdirs', () => {
      const root = join(workspaceRoot, '.kiro', 'specs');
      mkdirSync(root, { recursive: true });
      // Hidden directory that would otherwise look valid.
      mkdirSync(join(root, '.hidden'));
      writeFileSync(join(root, '.hidden', 'requirements.md'), 'x');
      // Plus a real one to confirm normal entries still come through.
      makeSpec('visible', ['requirements.md']);

      const features = listSpecFeatures(workspaceRoot);
      expect(features.map((f) => f.featureName)).toEqual(['visible']);
    });

    it('skips entries that are files, not directories', () => {
      const root = join(workspaceRoot, '.kiro', 'specs');
      mkdirSync(root, { recursive: true });
      writeFileSync(join(root, 'README.md'), 'top-level readme');
      makeSpec('real', ['tasks.md']);

      const features = listSpecFeatures(workspaceRoot);
      expect(features.map((f) => f.featureName)).toEqual(['real']);
    });

    it('returns features sorted alphabetically', () => {
      makeSpec('zeta', ['requirements.md']);
      makeSpec('alpha', ['requirements.md']);
      makeSpec('mu', ['requirements.md']);

      const names = listSpecFeatures(workspaceRoot).map((f) => f.featureName);
      expect(names).toEqual(['alpha', 'mu', 'zeta']);
    });

    it('returns empty array when readdir throws (e.g. permission denied)', () => {
      // Point at a regular file masquerading as the specs root: readdirSync
      // on a file throws ENOTDIR, which the helper must swallow.
      const fakeRoot = join(workspaceRoot, '.kiro', 'specs');
      mkdirSync(join(workspaceRoot, '.kiro'), { recursive: true });
      writeFileSync(fakeRoot, 'not a directory');

      expect(listSpecFeatures(workspaceRoot)).toEqual([]);
    });

    it('skips entries whose stat fails (broken symlink)', () => {
      const root = join(workspaceRoot, '.kiro', 'specs');
      mkdirSync(root, { recursive: true });
      // Symlink to a path that does not exist — statSync throws.
      try {
        symlinkSync(
          join(workspaceRoot, 'does-not-exist'),
          join(root, 'broken')
        );
      } catch {
        // Some filesystems (e.g. CI sandboxes) refuse symlinks. In that
        // case skip the test — the code path is still exercised by the
        // ENOTDIR scenario above.
        return;
      }
      makeSpec('healthy', ['requirements.md']);

      const features = listSpecFeatures(workspaceRoot);
      expect(features.map((f) => f.featureName)).toEqual(['healthy']);
    });
  });

  describe('findSpecFeature()', () => {
    it('returns the matching feature when present', () => {
      makeSpec('login', ['requirements.md']);
      makeSpec('signup', ['tasks.md']);

      const f = findSpecFeature(workspaceRoot, 'signup');
      expect(f?.featureName).toBe('signup');
      expect(f?.documents).toEqual(['tasks.md']);
    });

    it('returns undefined for unknown features', () => {
      makeSpec('login', ['requirements.md']);
      expect(findSpecFeature(workspaceRoot, 'missing')).toBeUndefined();
    });

    it('returns undefined when the specs root does not exist', () => {
      expect(findSpecFeature(workspaceRoot, 'anything')).toBeUndefined();
    });
  });

  describe('describeSpecDocuments()', () => {
    function summary(documents: SpecFeatureSummary['documents']) {
      return {
        featureName: 'x',
        dirPath: '/x',
        documents,
        specDocumentPaths: [],
      } as SpecFeatureSummary;
    }

    it('returns "empty" when no documents are present', () => {
      expect(describeSpecDocuments(summary([]))).toBe('empty');
    });

    it('joins document names with the .md suffix stripped', () => {
      expect(
        describeSpecDocuments(summary(['requirements.md', 'design.md']))
      ).toBe('requirements, design');
    });

    it('preserves the spec workflow order', () => {
      expect(
        describeSpecDocuments(
          summary(['requirements.md', 'design.md', 'tasks.md', 'bugfix.md'])
        )
      ).toBe('requirements, design, tasks, bugfix');
    });
  });
});
