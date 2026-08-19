import { afterEach, describe, expect, test } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { StorybookDefinition } from '../src/storybook/contracts.js';
import { stories } from '../src/storybook/stories.js';
import {
  collectVisualCoverage,
  visualCoverageHtml,
  visualCoverageMarkdown,
  visualCoverageSummaryMarkdown,
} from '../src/storybook/visual-coverage.js';

const fixtureRoots: string[] = [];
const sourceRoot = fileURLToPath(new URL('../src', import.meta.url));

afterEach(() => {
  for (const root of fixtureRoots.splice(0)) {
    fs.rmSync(root, { force: true, recursive: true });
  }
});

function sourceFixture(files: Readonly<Record<string, string>>): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'visual-coverage-'));
  fixtureRoots.push(root);
  for (const [relativePath, contents] of Object.entries(files)) {
    const filePath = path.join(root, relativePath);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, contents);
  }
  return root;
}

function fixtureStory(
  variants: StorybookDefinition['variants']
): StorybookDefinition {
  return {
    id: 'fixture-root',
    name: 'Root',
    description: 'Root fixture',
    category: 'Fixture',
    sourcePath: '../components/Root.stories.js',
    variants,
    component: null,
  };
}

describe('visual story coverage', () => {
  test('classifies covered, catalog-only, transitive, and uncovered components', () => {
    const root = sourceFixture({
      'components/Root.tsx': `
        import { Child } from './Child.js';
        export function Root() { return <Child />; }
      `,
      'components/Child.tsx':
        'export function Child() { return <box>child</box>; }',
      'components/Uncovered.tsx':
        'export function Uncovered() { return <box>uncovered</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Default = {};
      `,
    });
    const coverage = collectVisualCoverage(root, [
      fixtureStory([
        {
          id: 'default',
          name: 'Default',
          props: {},
          parameters: {
            certification: { suite: 'visual-stories', readyText: 'child' },
          },
        },
      ]),
    ]);

    expect(coverage).toMatchObject({
      totalStories: 1,
      coveredStories: 1,
      totalVariants: 1,
      executedVariants: 1,
      totalComponents: 3,
      coveredComponents: 2,
      catalogOnlyComponents: 0,
      uncoveredComponents: 1,
      storyCoveragePercent: 100,
      variantExecutionPercent: 100,
    });
    expect(coverage.componentCoveragePercent).toBeCloseTo(200 / 3);
    expect(
      coverage.components.find((component) => component.symbolName === 'Child')
        ?.classification
    ).toBe('covered');

    const otherSuite = collectVisualCoverage(
      root,
      [
        fixtureStory([
          {
            id: 'default',
            name: 'Default',
            props: {},
            parameters: {
              certification: { suite: 'visual-stories', readyText: 'child' },
            },
          },
        ]),
      ],
      'other-suite'
    );
    expect(otherSuite).toMatchObject({
      coveredStories: 0,
      executedVariants: 0,
      coveredComponents: 0,
      catalogOnlyComponents: 2,
      storyCoveragePercent: 0,
      variantExecutionPercent: 0,
      componentCoveragePercent: 0,
    });
  });

  test('reports covered and missed inventory with registration gaps', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Captured = {};
        export const Uncaptured = {};
      `,
      'components/Other.stories.tsx': 'export default {}; export const A = {};',
    });
    const coverage = collectVisualCoverage(root, [
      fixtureStory([
        {
          id: 'captured',
          name: 'Captured',
          props: {},
          parameters: {
            certification: { suite: 'visual-stories', readyText: 'root' },
          },
          play: async () => {},
        },
        {
          id: 'uncaptured',
          name: 'Uncaptured',
          props: {},
          parameters: {},
          play: async () => {},
        },
      ]),
    ]);

    expect(coverage).toMatchObject({
      totalStoryFiles: 2,
      registeredStoryFiles: 1,
      totalStories: 1,
      coveredStories: 1,
      totalVariants: 2,
      executedVariants: 1,
      storyCoveragePercent: 100,
      variantExecutionPercent: 50,
    });
    expect(coverage.unregisteredStoryFiles).toEqual([
      'components/Other.stories.tsx',
    ]);
    expect(coverage.variants).toEqual([
      expect.objectContaining({
        id: 'fixture-root/captured',
        status: 'executed',
      }),
      expect.objectContaining({
        id: 'fixture-root/uncaptured',
        status: 'missed',
      }),
    ]);

    const summary = visualCoverageSummaryMarkdown(coverage);
    expect(summary).toContain('| Story coverage | **100.0%** | 1 | 0 | 1 |');
    expect(summary).toContain('| Variant execution | **50.0%** | 1 | 1 | 2 |');
    expect(summary).not.toContain('interaction');

    const markdown = visualCoverageMarkdown(coverage);
    expect(markdown).toContain('### Executed (1)');
    expect(markdown).toContain('`fixture-root/captured`');
    expect(markdown).toContain('### Missed (1)');
    expect(markdown).toContain('`fixture-root/uncaptured`');

    const html = visualCoverageHtml(coverage);
    expect(html).toContain('Story coverage');
    expect(html).toContain('fixture-root/uncaptured');
    expect(html).toContain('Missed');
    expect(html).not.toContain('interaction');
  });

  test('enforces catalog registration without a percentage threshold', () => {
    const coverage = collectVisualCoverage(sourceRoot, stories);

    expect(coverage.totalComponents).toBeGreaterThan(0);
    expect(
      coverage.coveredComponents +
        coverage.catalogOnlyComponents +
        coverage.uncoveredComponents
    ).toBe(coverage.totalComponents);
    expect(coverage.unregisteredStoryFiles).toEqual([]);
    expect(coverage.registeredStoryFiles).toBe(coverage.totalStoryFiles);
    expect(coverage.coveredStories).toBeGreaterThan(0);
    expect(coverage.executedVariants).toBeGreaterThan(0);
    expect(coverage.stories).toHaveLength(coverage.totalStories);
    expect(coverage.variants).toHaveLength(coverage.totalVariants);
  });
});
