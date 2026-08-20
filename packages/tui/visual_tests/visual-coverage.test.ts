import { afterEach, describe, expect, test } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import type { StorybookDefinition } from '../src/storybook/contracts.js';
import { stories } from '../src/storybook/stories.js';
import {
  finalizeVisualEvidence,
  type VisualEvidenceCoverage,
} from '../src/storybook/visual-evidence.js';
import {
  collectVisualCoverage,
  STORYBOOK_CATALOG_SUITE,
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
  variants: StorybookDefinition['variants'],
  id = 'fixture-root'
): StorybookDefinition {
  return {
    id,
    name: 'Root',
    description: 'Root fixture',
    category: 'Fixture',
    sourcePath: '../components/Root.stories.js',
    variants,
    component: null,
  };
}

function capturedExecution(
  variantIds: readonly string[],
  captureIds = variantIds.map((variantId) => `${variantId}#default`)
) {
  return {
    successfulVariantIds: new Set(variantIds),
    successfulCaptureIds: new Set(captureIds),
  };
}

describe('visual story coverage', () => {
  test('persists planned evidence when captured coverage collection fails', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Default = {};
      `,
    });
    const definition = fixtureStory([
      {
        id: 'default',
        name: 'Default',
        props: {},
        parameters: {},
      },
    ]);
    const plannedCoverage = collectVisualCoverage(
      root,
      [definition],
      'planned'
    );
    let persisted: VisualEvidenceCoverage | undefined;

    const result = finalizeVisualEvidence(
      plannedCoverage,
      () => {
        throw new Error('covered state still declares a gap');
      },
      (evidence) => {
        persisted = evidence;
      }
    );

    expect(persisted).toEqual({
      coverage: plannedCoverage,
      collectionError: 'covered state still declares a gap',
    });
    expect(result).toEqual(persisted);
  });

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
    const coverage = collectVisualCoverage(
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
      capturedExecution(['fixture-root/default'])
    );

    expect(coverage).toMatchObject({
      totalStories: 1,
      coveredStories: 1,
      totalVariants: 1,
      executedVariants: 1,
      verifiedVariants: 0,
      totalComponents: 3,
      coveredComponents: 2,
      catalogOnlyComponents: 0,
      uncoveredComponents: 1,
      storyCoveragePercent: 100,
      variantExecutionPercent: 100,
      semanticVariantCoveragePercent: 0,
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
      capturedExecution(['fixture-root/default']),
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

    const catalogSuite = collectVisualCoverage(
      root,
      [
        fixtureStory([
          {
            id: 'default',
            name: 'Default',
            props: {},
            parameters: {},
          },
        ]),
      ],
      capturedExecution(['fixture-root/default']),
      STORYBOOK_CATALOG_SUITE
    );
    expect(catalogSuite).toMatchObject({
      coveredStories: 1,
      executedVariants: 1,
      coveredComponents: 2,
      storyCoveragePercent: 100,
      variantExecutionPercent: 100,
    });
  });

  test('scopes an ad hoc capture to one story and its full variant count', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const First = {};
        export const Second = {};
      `,
    });
    const variant = (id: string) => ({
      id,
      name: id,
      props: {},
      parameters: {},
    });
    const coverage = collectVisualCoverage(
      root,
      [
        fixtureStory([variant('first'), variant('second')], 'selected'),
        fixtureStory([variant('other')], 'other'),
      ],
      capturedExecution(['selected/first', 'other/other']),
      STORYBOOK_CATALOG_SUITE,
      'selected'
    );

    expect(coverage).toMatchObject({
      totalStories: 1,
      coveredStories: 1,
      totalVariants: 2,
      executedVariants: 1,
      failedVariants: 1,
      variantExecutionPercent: 50,
    });
    expect(coverage.stories.map((story) => story.storyId)).toEqual([
      'selected',
    ]);
    expect(
      coverage.variants.map((variantCoverage) => variantCoverage.id)
    ).toEqual(['selected/first', 'selected/second']);
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
    const coverage = collectVisualCoverage(
      root,
      [
        fixtureStory([
          {
            id: 'captured',
            name: 'Captured',
            props: {},
            parameters: {
              visualStates: {
                idle: { label: 'Idle state' },
                active: {
                  label: 'Active state',
                  gapType: 'missing-story',
                  description: 'No variant covers active yet',
                },
              },
              coversVisualStates: ['idle'],
              certification: {
                suite: 'visual-stories',
                readyText: 'root',
                assertions: { visible: ['root'] },
              },
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
      ],
      capturedExecution(['fixture-root/captured'])
    );

    expect(coverage).toMatchObject({
      totalStoryFiles: 2,
      registeredStoryFiles: 1,
      totalStories: 1,
      coveredStories: 1,
      totalVariants: 2,
      executedVariants: 1,
      totalVisualStates: 2,
      coveredVisualStates: 1,
      storyCoveragePercent: 100,
      variantExecutionPercent: 50,
      semanticVariantCoveragePercent: 50,
      visualStateCoveragePercent: 50,
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
    expect(summary).toContain(
      '| Semantic variant coverage | **50.0%** | 1 | 1 | 2 |'
    );
    expect(summary).toContain(
      '| Visual state coverage | **50.0%** | 1 | 1 | 2 |'
    );

    const markdown = visualCoverageMarkdown(coverage);
    expect(markdown).toContain('### Executed (1)');
    expect(markdown).toContain('`fixture-root/captured`');
    expect(markdown).toContain('### Missed (1)');
    expect(markdown).toContain('`fixture-root/uncaptured`');
    expect(markdown).toContain('`fixture-root/idle` - Idle state');
    expect(markdown).toContain('`fixture-root/active` - Active state');

    const html = visualCoverageHtml(coverage);
    expect(html).toContain('Story coverage');
    expect(html).toContain('fixture-root/uncaptured');
    expect(html).toContain('fixture-root/active');
    expect(html).toContain('No variant covers active yet');
    expect(html).toContain('missing story');
    expect(html).toContain('Asserted (1)');
    expect(html).toContain('Not run');
    expect(html).toContain('Missed');
  });

  test('rejects visual-state evidence that is not declared by the story', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Default = {};
      `,
    });

    expect(() =>
      collectVisualCoverage(
        root,
        [
          fixtureStory([
            {
              id: 'default',
              name: 'Default',
              props: {},
              parameters: { coversVisualStates: ['missing'] },
            },
          ]),
        ],
        'planned'
      )
    ).toThrow('covers undeclared visual state "missing"');
  });

  test('binds journey states to successful capture evidence', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Default = {};
      `,
    });
    const journey = {
      id: 'journey',
      name: 'Journey',
      props: {},
      parameters: {
        visualStates: { active: { label: 'Active state' } },
        coversVisualStates: ['active'],
        certification: {
          suite: 'visual-stories',
          readyText: 'root',
          assertions: { visible: ['root'] },
          captures: {
            active: {
              label: 'active capture',
              assertions: { visible: ['active'] },
            },
          },
        },
      },
    };

    expect(() =>
      collectVisualCoverage(root, [fixtureStory([journey])], 'planned')
    ).toThrow('must bind journey states to individual captures');

    expect(() =>
      collectVisualCoverage(
        root,
        [
          fixtureStory([
            {
              ...journey,
              parameters: {
                ...journey.parameters,
                coversVisualStates: [],
                certification: {
                  ...journey.parameters.certification,
                  captures: {
                    active: {
                      label: 'active capture',
                      coversVisualStates: ['active'],
                    },
                  },
                },
              },
            },
          ]),
        ],
        'planned'
      )
    ).toThrow('maps visual states without assertions');

    const coverage = collectVisualCoverage(
      root,
      [
        fixtureStory([
          {
            ...journey,
            parameters: {
              ...journey.parameters,
              coversVisualStates: [],
              certification: {
                ...journey.parameters.certification,
                captures: {
                  active: {
                    label: 'active capture',
                    assertions: { visible: ['active'] },
                    coversVisualStates: ['active'],
                  },
                },
              },
            },
          },
        ]),
      ],
      capturedExecution(
        ['fixture-root/journey'],
        ['fixture-root/journey#active']
      )
    );
    expect(coverage.visualStates).toEqual([
      expect.objectContaining({
        id: 'fixture-root/active',
        status: 'covered',
        evidence: ['journey#active'],
      }),
    ]);
  });

  test('derives positive coverage from successful capture results', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Default = {};
      `,
    });
    const coverage = collectVisualCoverage(
      root,
      [
        fixtureStory([
          {
            id: 'default',
            name: 'Default',
            props: {},
            parameters: {
              visualStates: { ready: { label: 'Ready state' } },
              coversVisualStates: ['ready'],
              certification: {
                suite: 'visual-stories',
                readyText: 'root',
                assertions: { visible: ['root'] },
              },
            },
          },
        ]),
      ],
      { successfulCaptureIds: new Set(), successfulVariantIds: new Set() },
      STORYBOOK_CATALOG_SUITE,
      undefined
    );

    expect(coverage).toMatchObject({
      coveredStories: 0,
      executedVariants: 0,
      failedVariants: 1,
      verifiedVariants: 0,
      coveredVisualStates: 0,
      coveredComponents: 0,
      catalogOnlyComponents: 1,
    });
    expect(coverage.variants[0]).toMatchObject({
      status: 'failed',
      verification: 'failed',
    });
    expect(coverage.stories[0]?.status).toBe('failed');
    expect(visualCoverageSummaryMarkdown(coverage)).toContain(
      'Failed variants: **1**'
    );
    expect(visualCoverageHtml(coverage)).toContain('Capture failed');
  });

  test('keeps passed journey-state evidence when a later capture fails', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Default = {};
      `,
    });
    const coverage = collectVisualCoverage(
      root,
      [
        fixtureStory([
          {
            id: 'journey',
            name: 'Journey',
            props: {},
            parameters: {
              visualStates: {
                selected: { label: 'Selected state' },
                submitted: { label: 'Submitted state' },
              },
              certification: {
                suite: 'visual-stories',
                readyText: 'root',
                captures: {
                  selected: {
                    label: 'selected capture',
                    assertions: { visible: ['selected'] },
                    coversVisualStates: ['selected'],
                  },
                  submitted: {
                    label: 'submitted capture',
                    assertions: { visible: ['submitted'] },
                    coversVisualStates: ['submitted'],
                  },
                },
              },
            },
          },
        ]),
      ],
      capturedExecution([], ['fixture-root/journey#selected'])
    );

    expect(coverage).toMatchObject({
      failedVariants: 1,
      coveredVisualStates: 1,
      failedVisualStates: 1,
    });
    expect(coverage.visualStates).toEqual([
      expect.objectContaining({
        id: 'fixture-root/selected',
        status: 'covered',
        evidence: ['journey#selected'],
      }),
      expect.objectContaining({
        id: 'fixture-root/submitted',
        status: 'failed',
        evidence: [],
      }),
    ]);
    expect(visualCoverageSummaryMarkdown(coverage)).toContain(
      'Failed visual states: **1**'
    );
    expect(visualCoverageSummaryMarkdown(coverage)).toContain(
      '**0 unclassified**'
    );
  });

  test('does not cover every story-file component when a sibling variant fails', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Passing = {};
        export const Failing = {};
      `,
    });
    const coverage = collectVisualCoverage(
      root,
      [
        fixtureStory([
          {
            id: 'passing',
            name: 'Passing',
            props: {},
            parameters: {
              certification: {
                suite: 'visual-stories',
                readyText: 'root',
              },
            },
          },
          {
            id: 'failing',
            name: 'Failing',
            props: {},
            parameters: {
              certification: {
                suite: 'visual-stories',
                readyText: 'root',
              },
            },
          },
        ]),
      ],
      capturedExecution(['fixture-root/passing'])
    );

    expect(coverage).toMatchObject({
      coveredStories: 1,
      executedVariants: 1,
      failedVariants: 1,
      coveredComponents: 0,
      catalogOnlyComponents: 1,
    });
  });

  test('scopes variants and states to a named suite', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Default = {};
      `,
    });
    const storyForSuite = (
      id: string,
      suite: string,
      stateId: string
    ): StorybookDefinition =>
      fixtureStory(
        [
          {
            id: 'default',
            name: 'Default',
            props: {},
            parameters: {
              visualStates: { [stateId]: { label: `${stateId} state` } },
              coversVisualStates: [stateId],
              certification: {
                suite,
                readyText: stateId,
                assertions: { visible: [stateId] },
              },
            },
          },
        ],
        id
      );

    const coverage = collectVisualCoverage(
      root,
      [
        storyForSuite('visual-story', 'visual-stories', 'visual'),
        storyForSuite('workflow-story', 'workflow-monitor', 'workflow'),
      ],
      capturedExecution(['visual-story/default']),
      'visual-stories'
    );
    expect(coverage).toMatchObject({
      totalStories: 1,
      coveredStories: 1,
      totalVariants: 1,
      executedVariants: 1,
      totalVisualStates: 1,
      coveredVisualStates: 1,
    });
    expect(coverage.visualStates.map((state) => state.id)).toEqual([
      'visual-story/visual',
    ]);
  });

  test('rejects stale or unexplained visual-state gaps', () => {
    const root = sourceFixture({
      'components/Root.tsx':
        'export function Root() { return <box>root</box>; }',
      'components/Root.stories.tsx': `
        import { Root } from './Root.js';
        export default { component: Root };
        export const Default = {};
      `,
    });
    const variant = {
      id: 'default',
      name: 'Default',
      props: {},
      parameters: {
        coversVisualStates: ['limited'],
        certification: {
          suite: 'visual-stories',
          readyText: 'root',
          assertions: { visible: ['root'] },
        },
      },
    };

    expect(() =>
      collectVisualCoverage(
        root,
        [
          fixtureStory([
            {
              ...variant,
              parameters: {
                ...variant.parameters,
                visualStates: {
                  limited: {
                    label: 'Limited state',
                    gapType: 'product-limitation',
                  },
                },
              },
            },
          ]),
        ],
        'planned'
      )
    ).toThrow('declares gap "product-limitation" without a description');

    expect(() =>
      collectVisualCoverage(
        root,
        [
          fixtureStory([
            {
              ...variant,
              parameters: {
                ...variant.parameters,
                visualStates: {
                  limited: {
                    label: 'Limited state',
                    gapType: 'product-limitation',
                    description: 'The component cannot render this state.',
                  },
                },
              },
            },
          ]),
        ],
        capturedExecution(['fixture-root/default'])
      )
    ).toThrow('declares gap "product-limitation" but is covered by: default');
  });

  test('validates planned catalog completeness without a percentage threshold', () => {
    const coverage = collectVisualCoverage(
      sourceRoot,
      stories,
      'planned',
      STORYBOOK_CATALOG_SUITE
    );

    expect(coverage.totalComponents).toBeGreaterThan(0);
    expect(
      coverage.coveredComponents +
        coverage.catalogOnlyComponents +
        coverage.uncoveredComponents
    ).toBe(coverage.totalComponents);
    expect(coverage.unregisteredStoryFiles).toEqual([]);
    expect(coverage.registeredStoryFiles).toBe(coverage.totalStoryFiles);
    expect(coverage).toMatchObject({
      mode: 'planned',
      coveredStories: 0,
      executedVariants: 0,
      failedVariants: 0,
      coveredComponents: 0,
      storyCoveragePercent: 0,
      variantExecutionPercent: 0,
    });
    expect(coverage.stories.every((story) => story.status === 'planned')).toBe(
      true
    );
    expect(
      coverage.variants.every((variant) => variant.status === 'planned')
    ).toBe(true);
    expect(coverage.stories).toHaveLength(coverage.totalStories);
    expect(coverage.variants).toHaveLength(coverage.totalVariants);
    expect(visualCoverageHtml(coverage)).toContain(
      'Planned catalog inventory without runtime evidence.'
    );
    expect(visualCoverageSummaryMarkdown(coverage)).toContain(
      '**Incomplete evidence:** This is planned catalog inventory.'
    );
    expect(visualCoverageSummaryMarkdown(coverage)).toContain(
      'zero failures do not indicate a passing visual suite'
    );
  });
});
