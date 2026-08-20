import * as fs from 'node:fs';
import * as path from 'node:path';
import ts from 'typescript';
import type { StorybookAssertions, StorybookDefinition } from './contracts.js';

export const STORYBOOK_CATALOG_SUITE = 'storybook-catalog';

export type VisualComponentClassification =
  | 'covered'
  | 'catalog-only'
  | 'uncovered';

export interface VisualStoryCoverage {
  storyId: string;
  storyName: string;
  sourcePath: string;
  status: 'covered' | 'failed' | 'planned' | 'missed';
  executedVariants: number;
  failedVariants: number;
  verifiedVariants: number;
  totalVariants: number;
  coveragePercent: number;
  semanticCoveragePercent: number;
}

export interface VisualVariantCoverage {
  id: string;
  storyId: string;
  storyName: string;
  variantId: string;
  variantName: string;
  status: 'executed' | 'failed' | 'planned' | 'missed';
  verification:
    | 'asserted'
    | 'capture-only'
    | 'failed'
    | 'planned-asserted'
    | 'planned-capture'
    | 'missed';
  assertionCount: number;
}

export interface VisualStateCoverage {
  id: string;
  storyId: string;
  storyName: string;
  stateId: string;
  label: string;
  description?: string;
  gapType?:
    | 'missing-story'
    | 'integration-only'
    | 'product-limitation'
    | 'visual-baseline-required';
  status: 'covered' | 'failed' | 'planned' | 'missed';
  evidence: readonly string[];
}

export interface VisualComponentCoverage {
  componentId: string;
  sourcePath: string;
  symbolName: string;
  exported: boolean;
  classification: VisualComponentClassification;
  evidence: readonly string[];
}

export interface VisualCoverage {
  mode: 'planned' | 'captured';
  totalStories: number;
  coveredStories: number;
  totalVariants: number;
  executedVariants: number;
  failedVariants: number;
  verifiedVariants: number;
  totalVisualStates: number;
  coveredVisualStates: number;
  failedVisualStates: number;
  totalStoryFiles: number;
  registeredStoryFiles: number;
  unregisteredStoryFiles: readonly string[];
  totalComponents: number;
  coveredComponents: number;
  catalogOnlyComponents: number;
  uncoveredComponents: number;
  storyCoveragePercent: number;
  variantExecutionPercent: number;
  semanticVariantCoveragePercent: number;
  visualStateCoveragePercent: number;
  componentCoveragePercent: number;
  stories: readonly VisualStoryCoverage[];
  variants: readonly VisualVariantCoverage[];
  visualStates: readonly VisualStateCoverage[];
  components: readonly VisualComponentCoverage[];
}

export interface VisualCoverageExecution {
  successfulVariantIds: ReadonlySet<string>;
  successfulCaptureIds: ReadonlySet<string>;
}

interface RenderableComponent {
  componentId: string;
  sourcePath: string;
  symbolName: string;
  exported: boolean;
  node: ts.Node;
  symbol: ts.Symbol | null;
}

function walk(directory: string): string[] {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    return entry.isDirectory() ? walk(entryPath) : [entryPath];
  });
}

function normalizedRelative(root: string, filePath: string): string {
  return path.relative(root, filePath).split(path.sep).join('/');
}

function componentId(sourcePath: string, symbolName: string): string {
  return `${sourcePath}#${symbolName}`;
}

function isPascalCase(value: string): boolean {
  return /^[A-Z][A-Za-z0-9]*$/.test(value);
}

function isCreateElementCall(node: ts.Node): node is ts.CallExpression {
  if (!ts.isCallExpression(node) || node.arguments.length === 0) return false;
  return (
    (ts.isIdentifier(node.expression) &&
      node.expression.text === 'createElement') ||
    (ts.isPropertyAccessExpression(node.expression) &&
      node.expression.name.text === 'createElement')
  );
}

function isComponentWrapperCall(node: ts.Node): node is ts.CallExpression {
  if (!ts.isCallExpression(node) || node.arguments.length === 0) return false;
  return (
    (ts.isIdentifier(node.expression) &&
      (node.expression.text === 'memo' ||
        node.expression.text === 'forwardRef')) ||
    (ts.isPropertyAccessExpression(node.expression) &&
      (node.expression.name.text === 'memo' ||
        node.expression.name.text === 'forwardRef'))
  );
}

function unwrapExpression(expression: ts.Expression): ts.Expression {
  let current = expression;
  while (
    ts.isAsExpression(current) ||
    ts.isSatisfiesExpression(current) ||
    ts.isParenthesizedExpression(current)
  ) {
    current = current.expression;
  }
  return current;
}

function containsRenderSyntax(node: ts.Node): boolean {
  let found = false;
  const visit = (candidate: ts.Node): void => {
    if (
      ts.isJsxElement(candidate) ||
      ts.isJsxSelfClosingElement(candidate) ||
      ts.isJsxFragment(candidate) ||
      isCreateElementCall(candidate)
    ) {
      found = true;
      return;
    }
    if (!found) ts.forEachChild(candidate, visit);
  };
  visit(node);
  return found;
}

function declarationRenders(
  node: ts.Node,
  checker: ts.TypeChecker,
  sourceFile: ts.SourceFile
): boolean {
  if (containsRenderSyntax(node)) return true;
  if (!ts.isVariableDeclaration(node) || !node.initializer) return false;
  const initializer = unwrapExpression(node.initializer);
  if (!isComponentWrapperCall(initializer)) return false;
  const implementation = unwrapExpression(initializer.arguments[0]!);
  if (containsRenderSyntax(implementation)) return true;
  const symbol = canonicalSymbol(
    checker,
    checker.getSymbolAtLocation(implementation)
  );
  return (symbol?.declarations ?? []).some(
    (declaration) =>
      declaration.getSourceFile() === sourceFile &&
      containsRenderSyntax(declaration)
  );
}

function hasExportModifier(node: ts.Node): boolean {
  return (
    ts.canHaveModifiers(node) &&
    (ts
      .getModifiers(node)
      ?.some((modifier) => modifier.kind === ts.SyntaxKind.ExportKeyword) ??
      false)
  );
}

function canonicalSymbol(
  checker: ts.TypeChecker,
  symbol: ts.Symbol | undefined
): ts.Symbol | null {
  if (!symbol) return null;
  let current = symbol;
  const visited = new Set<ts.Symbol>();
  while (current.flags & ts.SymbolFlags.Alias && !visited.has(current)) {
    visited.add(current);
    current = checker.getAliasedSymbol(current);
  }
  return current;
}

function createProgram(sourceRoot: string): ts.Program {
  const rootNames = walk(sourceRoot).filter(
    (filePath) =>
      /\.(?:ts|tsx)$/.test(filePath) &&
      !filePath.endsWith('.d.ts') &&
      !filePath.split(path.sep).includes('node_modules')
  );
  return ts.createProgram({
    rootNames,
    options: {
      allowJs: true,
      jsx: ts.JsxEmit.ReactJSX,
      module: ts.ModuleKind.ESNext,
      moduleResolution: ts.ModuleResolutionKind.Bundler,
      skipLibCheck: true,
      target: ts.ScriptTarget.ESNext,
    },
  });
}

function discoverComponents(
  sourceRoot: string,
  program: ts.Program
): RenderableComponent[] {
  const checker = program.getTypeChecker();
  const componentRoot = path.join(sourceRoot, 'components');
  const files = walk(componentRoot).filter(
    (filePath) =>
      /\.(?:ts|tsx)$/.test(filePath) &&
      !/\.(?:stories|test|vitest)\.(?:ts|tsx)$/.test(filePath) &&
      !filePath.split(path.sep).includes('__tests__')
  );
  const components: RenderableComponent[] = [];

  for (const filePath of files) {
    const sourceFile = program.getSourceFile(filePath);
    if (!sourceFile) continue;
    const sourcePath = normalizedRelative(sourceRoot, filePath);

    for (const statement of sourceFile.statements) {
      if (
        ts.isFunctionDeclaration(statement) &&
        statement.name &&
        isPascalCase(statement.name.text) &&
        statement.body &&
        containsRenderSyntax(statement.body)
      ) {
        components.push({
          componentId: componentId(sourcePath, statement.name.text),
          sourcePath,
          symbolName: statement.name.text,
          exported: hasExportModifier(statement),
          node: statement,
          symbol: checker.getSymbolAtLocation(statement.name) ?? null,
        });
      }

      if (ts.isVariableStatement(statement)) {
        for (const declaration of statement.declarationList.declarations) {
          if (
            !ts.isIdentifier(declaration.name) ||
            !isPascalCase(declaration.name.text) ||
            !declaration.initializer ||
            !declarationRenders(declaration, checker, sourceFile)
          ) {
            continue;
          }
          components.push({
            componentId: componentId(sourcePath, declaration.name.text),
            sourcePath,
            symbolName: declaration.name.text,
            exported: hasExportModifier(statement),
            node: declaration,
            symbol: checker.getSymbolAtLocation(declaration.name) ?? null,
          });
        }
      }

      if (
        ts.isClassDeclaration(statement) &&
        statement.name &&
        isPascalCase(statement.name.text) &&
        statement.members.some(
          (member) =>
            ts.isMethodDeclaration(member) &&
            member.name.getText(sourceFile) === 'render' &&
            containsRenderSyntax(member)
        )
      ) {
        components.push({
          componentId: componentId(sourcePath, statement.name.text),
          sourcePath,
          symbolName: statement.name.text,
          exported: hasExportModifier(statement),
          node: statement,
          symbol: checker.getSymbolAtLocation(statement.name) ?? null,
        });
      }
    }
  }

  return components.sort((left, right) =>
    left.componentId.localeCompare(right.componentId)
  );
}

function collectComponentReferences(
  checker: ts.TypeChecker,
  bySymbol: ReadonlyMap<ts.Symbol, string>,
  node: ts.Node
): Set<string> {
  const references = new Set<string>();
  const inspectExpression = (expression: ts.Expression): void => {
    const symbol = canonicalSymbol(
      checker,
      checker.getSymbolAtLocation(unwrapExpression(expression))
    );
    const component = symbol ? bySymbol.get(symbol) : undefined;
    if (component) references.add(component);
  };
  const visit = (candidate: ts.Node): void => {
    if (
      ts.isJsxOpeningElement(candidate) ||
      ts.isJsxSelfClosingElement(candidate)
    ) {
      if (!ts.isJsxNamespacedName(candidate.tagName)) {
        inspectExpression(candidate.tagName);
      }
    } else if (
      ts.isJsxAttribute(candidate) &&
      candidate.initializer &&
      ts.isJsxExpression(candidate.initializer) &&
      candidate.initializer.expression
    ) {
      inspectExpression(candidate.initializer.expression);
    } else if (isCreateElementCall(candidate)) {
      inspectExpression(candidate.arguments[0]!);
    } else if (
      ts.isPropertyAssignment(candidate) &&
      candidate.name.getText(candidate.getSourceFile()).replace(/['"]/g, '') ===
        'component'
    ) {
      inspectExpression(candidate.initializer);
    }
    ts.forEachChild(candidate, visit);
  };
  visit(node);
  return references;
}

function propagateEvidence(
  roots: ReadonlyMap<string, ReadonlySet<string>>,
  dependencies: ReadonlyMap<string, ReadonlySet<string>>
): ReadonlyMap<string, ReadonlySet<string>> {
  const evidence = new Map<string, Set<string>>();
  const queue: Array<{ component: string; story: string }> = [];
  for (const [component, stories] of roots) {
    for (const story of stories) queue.push({ component, story });
  }

  while (queue.length > 0) {
    const current = queue.shift()!;
    const currentEvidence = evidence.get(current.component) ?? new Set();
    if (currentEvidence.has(current.story)) continue;
    currentEvidence.add(current.story);
    evidence.set(current.component, currentEvidence);
    for (const dependency of dependencies.get(current.component) ?? []) {
      queue.push({ component: dependency, story: current.story });
    }
  }
  return evidence;
}

function resolveStoryFile(
  sourceRoot: string,
  story: StorybookDefinition
): string | null {
  const unresolved = path.resolve(sourceRoot, 'storybook', story.sourcePath);
  const stem = unresolved.replace(/\.js$/, '');
  return (
    [`${stem}.tsx`, `${stem}.ts`].find((candidate) =>
      fs.existsSync(candidate)
    ) ?? null
  );
}

function percent(covered: number, total: number): number {
  return total === 0 ? 0 : (covered / total) * 100;
}

function assertionCount(assertions: StorybookAssertions | undefined): number {
  return (
    (assertions?.visible?.length ?? 0) +
    (assertions?.hidden?.filter((value) => value !== 'undefined').length ?? 0) +
    (assertions?.ordered?.length ?? 0) +
    Object.keys(assertions?.occurrences ?? {}).length +
    (assertions?.styled?.length ?? 0)
  );
}

function variantAssertionCount(
  variant: StorybookDefinition['variants'][number]
): number {
  const certification = variant.parameters.certification;
  return (
    assertionCount(certification?.assertions) +
    Object.values(certification?.captures ?? {}).reduce(
      (total, capture) => total + assertionCount(capture.assertions),
      0
    )
  );
}

function variantCoverageId(storyId: string, variantId: string): string {
  return `${storyId}/${variantId}`;
}

function captureCoverageId(
  storyId: string,
  variantId: string,
  captureId: string
): string {
  return `${variantCoverageId(storyId, variantId)}#${captureId}`;
}

function variantSucceeded(
  storyId: string,
  variant: StorybookDefinition['variants'][number],
  execution: VisualCoverageExecution | 'planned'
): boolean {
  return (
    execution !== 'planned' &&
    execution.successfulVariantIds.has(variantCoverageId(storyId, variant.id))
  );
}

function visualStateBindings(
  variant: StorybookDefinition['variants'][number],
  stateId: string
): string[] {
  const certification = variant.parameters.certification;
  const captures = certification?.captures;
  if (captures) {
    return Object.entries(captures).flatMap(([captureId, capture]) =>
      capture.coversVisualStates?.includes(stateId) &&
      assertionCount(capture.assertions) > 0
        ? [`${variant.id}#${captureId}`]
        : []
    );
  }
  return variant.parameters.coversVisualStates?.includes(stateId) &&
    assertionCount(certification?.assertions) > 0
    ? [variant.id]
    : [];
}

function visualStateEvidence(
  storyId: string,
  variant: StorybookDefinition['variants'][number],
  stateId: string,
  execution: VisualCoverageExecution | 'planned'
): string[] {
  if (execution === 'planned') return [];
  return visualStateBindings(variant, stateId).filter((binding) => {
    const separator = binding.indexOf('#');
    if (separator === -1) {
      return execution.successfulVariantIds.has(
        variantCoverageId(storyId, binding)
      );
    }
    return execution.successfulCaptureIds.has(
      captureCoverageId(
        storyId,
        binding.slice(0, separator),
        binding.slice(separator + 1)
      )
    );
  });
}

export function isVariantSelectedForVisualSuite(
  variant: StorybookDefinition['variants'][number],
  suite?: string
): boolean {
  if (suite === STORYBOOK_CATALOG_SUITE) return true;
  const certification = variant.parameters.certification;
  return (
    certification !== undefined &&
    (suite === undefined || certification.suite === suite)
  );
}

export function collectVisualCoverage(
  sourceRoot: string,
  stories: readonly StorybookDefinition[],
  execution: VisualCoverageExecution | 'planned',
  suite?: string,
  storyId?: string
): VisualCoverage {
  const normalizedRoot = path.resolve(sourceRoot);
  const scopedStories = stories.flatMap((story) => {
    if (storyId !== undefined && story.id !== storyId) return [];
    const variants =
      suite !== undefined && suite !== STORYBOOK_CATALOG_SUITE
        ? story.variants.filter((variant) =>
            isVariantSelectedForVisualSuite(variant, suite)
          )
        : story.variants;
    return variants.length > 0 ? [{ story, variants }] : [];
  });
  const scopedVariantsByStory = new Map(
    scopedStories.map(({ story, variants }) => [story.id, variants] as const)
  );
  const program = createProgram(normalizedRoot);
  const checker = program.getTypeChecker();
  const components = discoverComponents(normalizedRoot, program);
  const bySymbol = new Map<ts.Symbol, string>();
  for (const component of components) {
    const symbol = canonicalSymbol(checker, component.symbol ?? undefined);
    if (symbol) bySymbol.set(symbol, component.componentId);
  }

  const dependencies = new Map<string, ReadonlySet<string>>();
  for (const component of components) {
    const references = collectComponentReferences(
      checker,
      bySymbol,
      component.node
    );
    references.delete(component.componentId);
    dependencies.set(component.componentId, references);
  }

  const catalogRoots = new Map<string, Set<string>>();
  const coveredRoots = new Map<string, Set<string>>();
  const registeredStoryFiles = new Set<string>();
  for (const story of stories) {
    const storyFilePath = resolveStoryFile(normalizedRoot, story);
    if (!storyFilePath) {
      throw new Error(
        `Registered Storybook story "${story.id}" has no source file for "${story.sourcePath}"`
      );
    }
    registeredStoryFiles.add(normalizedRelative(normalizedRoot, storyFilePath));
    const sourceFile = program.getSourceFile(storyFilePath);
    if (!sourceFile) {
      throw new Error(`Story source is missing from the TypeScript program`);
    }
    const subjects = collectComponentReferences(checker, bySymbol, sourceFile);
    for (const subject of subjects) {
      const catalog = catalogRoots.get(subject) ?? new Set<string>();
      catalog.add(story.id);
      catalogRoots.set(subject, catalog);
      const scopedVariants = scopedVariantsByStory.get(story.id) ?? [];
      if (
        scopedVariants.length > 0 &&
        scopedVariants.every((variant) =>
          variantSucceeded(story.id, variant, execution)
        )
      ) {
        const covered = coveredRoots.get(subject) ?? new Set<string>();
        covered.add(story.id);
        coveredRoots.set(subject, covered);
      }
    }
  }

  const catalogEvidence = propagateEvidence(catalogRoots, dependencies);
  const coveredEvidence = propagateEvidence(coveredRoots, dependencies);
  const componentCoverage = components.map(
    (component): VisualComponentCoverage => {
      const covered = coveredEvidence.get(component.componentId);
      if (covered) {
        return {
          componentId: component.componentId,
          sourcePath: component.sourcePath,
          symbolName: component.symbolName,
          exported: component.exported,
          classification: 'covered',
          evidence: [...covered].sort(),
        };
      }
      const catalog = catalogEvidence.get(component.componentId);
      if (catalog) {
        return {
          componentId: component.componentId,
          sourcePath: component.sourcePath,
          symbolName: component.symbolName,
          exported: component.exported,
          classification: 'catalog-only',
          evidence: [...catalog].sort(),
        };
      }
      return {
        componentId: component.componentId,
        sourcePath: component.sourcePath,
        symbolName: component.symbolName,
        exported: component.exported,
        classification: 'uncovered',
        evidence: [],
      };
    }
  );

  const storyFiles = walk(path.join(normalizedRoot, 'components'))
    .filter((filePath) => /\.stories\.(?:ts|tsx)$/.test(filePath))
    .map((filePath) => normalizedRelative(normalizedRoot, filePath))
    .sort();
  const registeredStoryFileCount = storyFiles.filter((storyFile) =>
    registeredStoryFiles.has(storyFile)
  ).length;
  const storyCoverage = scopedStories
    .map(({ story, variants }): VisualStoryCoverage => {
      const selectedVariants = variants.filter((variant) =>
        isVariantSelectedForVisualSuite(variant, suite)
      );
      const executedVariants = selectedVariants.filter((variant) =>
        variantSucceeded(story.id, variant, execution)
      ).length;
      const failedVariants =
        execution === 'planned'
          ? 0
          : selectedVariants.length - executedVariants;
      const verifiedVariants = selectedVariants.filter(
        (variant) =>
          variantSucceeded(story.id, variant, execution) &&
          variantAssertionCount(variant) > 0
      ).length;
      return {
        storyId: story.id,
        storyName: story.name,
        sourcePath: story.sourcePath,
        status:
          execution === 'planned'
            ? 'planned'
            : executedVariants > 0
              ? 'covered'
              : failedVariants > 0
                ? 'failed'
                : 'missed',
        executedVariants,
        failedVariants,
        verifiedVariants,
        totalVariants: variants.length,
        coveragePercent: percent(executedVariants, variants.length),
        semanticCoveragePercent: percent(verifiedVariants, variants.length),
      };
    })
    .sort((left, right) => left.storyId.localeCompare(right.storyId));
  const variantCoverage = scopedStories
    .flatMap(({ story, variants }) =>
      variants.map((variant): VisualVariantCoverage => {
        const selected = isVariantSelectedForVisualSuite(variant, suite);
        const succeeded =
          selected && variantSucceeded(story.id, variant, execution);
        const assertions = variantAssertionCount(variant);
        return {
          id: variantCoverageId(story.id, variant.id),
          storyId: story.id,
          storyName: story.name,
          variantId: variant.id,
          variantName: variant.name,
          status: !selected
            ? 'missed'
            : execution === 'planned'
              ? 'planned'
              : succeeded
                ? 'executed'
                : 'failed',
          verification: !selected
            ? 'missed'
            : execution === 'planned'
              ? assertions > 0
                ? 'planned-asserted'
                : 'planned-capture'
              : !succeeded
                ? 'failed'
                : assertions > 0
                  ? 'asserted'
                  : 'capture-only',
          assertionCount: assertions,
        };
      })
    )
    .sort((left, right) => left.id.localeCompare(right.id));
  const visualStateCoverage = scopedStories
    .flatMap(({ story, variants }): VisualStateCoverage[] => {
      const declaredStates =
        variants.find(
          (variant) => variant.parameters.visualStates !== undefined
        )?.parameters.visualStates ?? {};
      const declaredIds = new Set(Object.keys(declaredStates));
      for (const variant of story.variants) {
        const captures = variant.parameters.certification?.captures;
        if (
          captures &&
          (variant.parameters.coversVisualStates?.length ?? 0) > 0
        ) {
          throw new Error(
            `Story "${story.id}" variant "${variant.id}" must bind journey states to individual captures`
          );
        }
        for (const state of variant.parameters.coversVisualStates ?? []) {
          if (!declaredIds.has(state)) {
            throw new Error(
              `Story "${story.id}" variant "${variant.id}" covers undeclared visual state "${state}"`
            );
          }
        }
        for (const [captureId, capture] of Object.entries(captures ?? {})) {
          if (
            (capture.coversVisualStates?.length ?? 0) > 0 &&
            assertionCount(capture.assertions) === 0
          ) {
            throw new Error(
              `Story "${story.id}" variant "${variant.id}" capture "${captureId}" maps visual states without assertions`
            );
          }
          for (const state of capture.coversVisualStates ?? []) {
            if (!declaredIds.has(state)) {
              throw new Error(
                `Story "${story.id}" variant "${variant.id}" capture "${captureId}" covers undeclared visual state "${state}"`
              );
            }
          }
        }
      }
      return Object.entries(declaredStates).map(
        ([stateId, definition]): VisualStateCoverage => {
          const evidence = variants
            .filter((variant) =>
              isVariantSelectedForVisualSuite(variant, suite)
            )
            .flatMap((variant) =>
              visualStateEvidence(story.id, variant, stateId, execution)
            )
            .sort();
          const hasBindings = variants.some(
            (variant) =>
              isVariantSelectedForVisualSuite(variant, suite) &&
              visualStateBindings(variant, stateId).length > 0
          );
          if (definition.gapType && !definition.description?.trim()) {
            throw new Error(
              `Story "${story.id}" visual state "${stateId}" declares gap "${definition.gapType}" without a description`
            );
          }
          if (definition.gapType && evidence.length > 0) {
            throw new Error(
              `Story "${story.id}" visual state "${stateId}" declares gap "${definition.gapType}" but is covered by: ${evidence.join(', ')}`
            );
          }
          return {
            id: `${story.id}/${stateId}`,
            storyId: story.id,
            storyName: story.name,
            stateId,
            label: definition.label,
            ...(definition.description
              ? { description: definition.description }
              : {}),
            ...(definition.gapType ? { gapType: definition.gapType } : {}),
            status:
              evidence.length > 0
                ? 'covered'
                : execution === 'planned' && hasBindings
                  ? 'planned'
                  : execution !== 'planned' && hasBindings
                    ? 'failed'
                    : 'missed',
            evidence,
          };
        }
      );
    })
    .sort((left, right) => left.id.localeCompare(right.id));
  const coveredStories = storyCoverage.filter(
    (story) => story.status === 'covered'
  ).length;
  const executedVariants = variantCoverage.filter(
    (variant) => variant.status === 'executed'
  ).length;
  const failedVariants = variantCoverage.filter(
    (variant) => variant.status === 'failed'
  ).length;
  const verifiedVariants = variantCoverage.filter(
    (variant) => variant.verification === 'asserted'
  ).length;
  const coveredVisualStates = visualStateCoverage.filter(
    (state) => state.status === 'covered'
  ).length;
  const failedVisualStates = visualStateCoverage.filter(
    (state) => state.status === 'failed'
  ).length;
  const coveredComponents = componentCoverage.filter(
    (component) => component.classification === 'covered'
  ).length;
  const catalogOnlyComponents = componentCoverage.filter(
    (component) => component.classification === 'catalog-only'
  ).length;
  const uncoveredComponents =
    componentCoverage.length - coveredComponents - catalogOnlyComponents;

  return {
    mode: execution === 'planned' ? 'planned' : 'captured',
    totalStories: storyCoverage.length,
    coveredStories,
    totalVariants: variantCoverage.length,
    executedVariants,
    failedVariants,
    verifiedVariants,
    totalVisualStates: visualStateCoverage.length,
    coveredVisualStates,
    failedVisualStates,
    totalStoryFiles: storyFiles.length,
    registeredStoryFiles: registeredStoryFileCount,
    unregisteredStoryFiles: storyFiles.filter(
      (storyFile) => !registeredStoryFiles.has(storyFile)
    ),
    totalComponents: componentCoverage.length,
    coveredComponents,
    catalogOnlyComponents,
    uncoveredComponents,
    storyCoveragePercent: percent(coveredStories, storyCoverage.length),
    variantExecutionPercent: percent(executedVariants, variantCoverage.length),
    semanticVariantCoveragePercent: percent(
      verifiedVariants,
      variantCoverage.length
    ),
    visualStateCoveragePercent: percent(
      coveredVisualStates,
      visualStateCoverage.length
    ),
    componentCoveragePercent: percent(
      coveredComponents,
      componentCoverage.length
    ),
    stories: storyCoverage,
    variants: variantCoverage,
    visualStates: visualStateCoverage,
    components: componentCoverage,
  };
}

function formatPercent(value: number): string {
  return `${value.toFixed(1)}%`;
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function visualGapCounts(coverage: VisualCoverage): {
  missingStory: number;
  integrationOnly: number;
  productLimitation: number;
  visualBaselineRequired: number;
  unclassified: number;
} {
  const missed = coverage.visualStates.filter(
    (state) => state.status === 'missed'
  );
  return {
    missingStory: missed.filter((state) => state.gapType === 'missing-story')
      .length,
    integrationOnly: missed.filter(
      (state) => state.gapType === 'integration-only'
    ).length,
    productLimitation: missed.filter(
      (state) => state.gapType === 'product-limitation'
    ).length,
    visualBaselineRequired: missed.filter(
      (state) => state.gapType === 'visual-baseline-required'
    ).length,
    unclassified: missed.filter((state) => state.gapType === undefined).length,
  };
}

function coverageSummaryMarkdown(coverage: VisualCoverage): string {
  const gaps = visualGapCounts(coverage);
  const evidenceNotice =
    coverage.mode === 'planned'
      ? '> **Incomplete evidence:** This is planned catalog inventory. No runtime captures were applied, so zero failures do not indicate a passing visual suite.\n\n'
      : '';
  return `# Visual Stories Coverage

${evidenceNotice}| Metric | Coverage | Covered | Not covered | Total |
| --- | ---: | ---: | ---: | ---: |
| Story coverage | **${formatPercent(coverage.storyCoveragePercent)}** | ${coverage.coveredStories} | ${coverage.totalStories - coverage.coveredStories} | ${coverage.totalStories} |
| Variant execution | **${formatPercent(coverage.variantExecutionPercent)}** | ${coverage.executedVariants} | ${coverage.totalVariants - coverage.executedVariants} | ${coverage.totalVariants} |
| Semantic variant coverage | **${formatPercent(coverage.semanticVariantCoveragePercent)}** | ${coverage.verifiedVariants} | ${coverage.totalVariants - coverage.verifiedVariants} | ${coverage.totalVariants} |
| Visual state coverage | **${formatPercent(coverage.visualStateCoveragePercent)}** | ${coverage.coveredVisualStates} | ${coverage.totalVisualStates - coverage.coveredVisualStates} | ${coverage.totalVisualStates} |
| Component coverage | **${formatPercent(coverage.componentCoveragePercent)}** | ${coverage.coveredComponents} | ${coverage.totalComponents - coverage.coveredComponents} | ${coverage.totalComponents} |

Failed variants: **${coverage.failedVariants}**.

Failed visual states: **${coverage.failedVisualStates}**.

Open visual-state gaps: **${gaps.missingStory} missing stories**, **${gaps.integrationOnly} integration-only**, **${gaps.productLimitation} product limitations**, **${gaps.visualBaselineRequired} visual-baseline-required**, **${gaps.unclassified} unclassified**.

*Story coverage: a story is covered when at least one in-scope variant completes successfully.*

*Variant execution: the share of in-scope variants that complete without capture or assertion failures.*

*Semantic variant coverage: the share of successful variants with explicit visible, hidden, ordering, or occurrence assertions.*

*Visual state coverage: the share of explicitly declared UI states proved by a successful variant or named capture.*

*Component coverage: a static estimate of components reachable from stories with successful captures; conditional branches may be counted even when they do not render in a captured frame.*

Story catalog: ${coverage.registeredStoryFiles}/${coverage.totalStoryFiles} files registered. Coverage percentages are informational and do not enforce a threshold.
`;
}

function markdownList(lines: readonly string[]): string {
  return lines.length === 0 ? 'None.' : lines.join('\n');
}

export function visualCoverageSummaryMarkdown(
  coverage: VisualCoverage
): string {
  return `${coverageSummaryMarkdown(coverage)}\n`;
}

export function visualCoverageMarkdown(coverage: VisualCoverage): string {
  const coveredStories = coverage.stories
    .filter((story) => story.status === 'covered')
    .map(
      (story) =>
        `- \`${story.storyId}\` - ${story.executedVariants}/${story.totalVariants} passed; ${story.failedVariants} failed; ${story.verifiedVariants}/${story.totalVariants} asserted`
    );
  const missedStories = coverage.stories
    .filter((story) => story.status === 'missed')
    .map(
      (story) =>
        `- \`${story.storyId}\` - 0/${story.totalVariants} variants executed`
    );
  const failedStories = coverage.stories
    .filter((story) => story.status === 'failed')
    .map(
      (story) =>
        `- \`${story.storyId}\` - ${story.failedVariants}/${story.totalVariants} variants failed`
    );
  const plannedStories = coverage.stories
    .filter((story) => story.status === 'planned')
    .map(
      (story) =>
        `- \`${story.storyId}\` - ${story.totalVariants} variants planned`
    );
  const executedVariants = coverage.variants
    .filter((variant) => variant.status === 'executed')
    .map(
      (variant) =>
        `- \`${variant.id}\` - ${variant.verification === 'asserted' ? `${variant.assertionCount} assertions` : 'capture only'}`
    );
  const missedVariants = coverage.variants
    .filter((variant) => variant.status === 'missed')
    .map((variant) => `- \`${variant.id}\``);
  const failedVariants = coverage.variants
    .filter((variant) => variant.status === 'failed')
    .map((variant) => `- \`${variant.id}\``);
  const plannedVariants = coverage.variants
    .filter((variant) => variant.status === 'planned')
    .map((variant) => `- \`${variant.id}\``);
  const coveredVisualStates = coverage.visualStates
    .filter((state) => state.status === 'covered')
    .map(
      (state) =>
        `- \`${state.id}\` - ${state.label}; evidence: ${state.evidence.join(', ')}`
    );
  const missedVisualStates = coverage.visualStates
    .filter((state) => state.status === 'missed')
    .map(
      (state) =>
        `- \`${state.id}\` - ${state.label}${state.gapType ? ` [${state.gapType}]` : ''}${state.description ? `; ${state.description}` : ''}`
    );
  const failedVisualStates = coverage.visualStates
    .filter((state) => state.status === 'failed')
    .map((state) => `- \`${state.id}\` - ${state.label}`);
  const plannedVisualStates = coverage.visualStates
    .filter((state) => state.status === 'planned')
    .map((state) => `- \`${state.id}\` - ${state.label}`);
  const coveredComponents = coverage.components
    .filter((component) => component.classification === 'covered')
    .map(
      (component) =>
        `- \`${component.componentId}\` - ${component.evidence.join(', ')}`
    );
  const missedComponents = coverage.components
    .filter((component) => component.classification !== 'covered')
    .map((component) =>
      component.classification === 'catalog-only'
        ? `- \`${component.componentId}\` - catalog story only: ${component.evidence.join(', ')}`
        : `- \`${component.componentId}\` - no registered story`
    );

  return `${coverageSummaryMarkdown(coverage)}

## Story Detail

### Covered (${coveredStories.length})

${markdownList(coveredStories)}

### Missed (${missedStories.length})

${markdownList(missedStories)}

### Failed (${failedStories.length})

${markdownList(failedStories)}

### Planned (${plannedStories.length})

${markdownList(plannedStories)}

## Variant Detail

### Executed (${executedVariants.length})

${markdownList(executedVariants)}

### Failed (${failedVariants.length})

${markdownList(failedVariants)}

### Missed (${missedVariants.length})

${markdownList(missedVariants)}

### Planned (${plannedVariants.length})

${markdownList(plannedVariants)}

## Visual State Detail

### Covered (${coveredVisualStates.length})

${markdownList(coveredVisualStates)}

### Missed (${missedVisualStates.length})

${markdownList(missedVisualStates)}

### Failed (${failedVisualStates.length})

${markdownList(failedVisualStates)}

### Planned (${plannedVisualStates.length})

${markdownList(plannedVisualStates)}

## Component Detail

### Covered (${coveredComponents.length})

${markdownList(coveredComponents)}

### Missed (${missedComponents.length})

${markdownList(missedComponents)}
`;
}

function coverageMetricHtml(
  label: string,
  covered: number,
  total: number,
  coveragePercent: number,
  description: string,
  coveredTerm = 'covered'
): string {
  const notCovered = total - covered;
  return `<article class="coverage-row">
  <div class="coverage-name"><h2>${escapeHtml(label)}</h2><p>${covered} ${coveredTerm} · ${notCovered} not covered · ${total} total</p></div>
  <div class="coverage-track" role="img" aria-label="${escapeHtml(`${label}: ${formatPercent(coveragePercent)}, ${covered} ${coveredTerm}, ${notCovered} not covered, ${total} total`)}"><span style="width:${coveragePercent.toFixed(1)}%"></span></div>
  <strong>${formatPercent(coveragePercent)}</strong>
  <p class="coverage-note">${escapeHtml(description)}</p>
</article>`;
}

function statusLabel(component: VisualComponentCoverage): {
  label: string;
  className: string;
  evidence: string;
} {
  if (component.classification === 'covered') {
    return {
      label: 'Covered',
      className: 'covered',
      evidence: component.evidence.join(', '),
    };
  }
  if (component.classification === 'catalog-only') {
    return {
      label: 'Missed - catalog only',
      className: 'missed',
      evidence: component.evidence.join(', '),
    };
  }
  return {
    label: 'Missed - no story',
    className: 'missed',
    evidence: 'none',
  };
}

export function visualCoverageHtml(coverage: VisualCoverage): string {
  const gaps = visualGapCounts(coverage);
  const plannedStories = coverage.stories.filter(
    (story) => story.status === 'planned'
  ).length;
  const failedStories = coverage.stories.filter(
    (story) => story.status === 'failed'
  ).length;
  const plannedVariants = coverage.variants.filter(
    (variant) => variant.status === 'planned'
  ).length;
  const plannedVisualStates = coverage.visualStates.filter(
    (state) => state.status === 'planned'
  ).length;
  const storyRows = [...coverage.stories]
    .sort(
      (left, right) =>
        Number(right.status === 'covered') -
          Number(left.status === 'covered') ||
        left.storyId.localeCompare(right.storyId)
    )
    .map(
      (story) => `<tr>
<th scope="row"><code>${escapeHtml(story.storyId)}</code><span>${escapeHtml(story.storyName)}</span></th>
<td><span class="status ${story.status}">${story.status === 'covered' ? 'Covered' : story.status === 'failed' ? 'Failed' : story.status === 'planned' ? 'Planned' : 'Missed'}</span></td>
<td class="num">${story.executedVariants}/${story.totalVariants}</td>
<td class="num">${story.failedVariants}</td>
<td class="num">${story.verifiedVariants}/${story.totalVariants}</td>
<td class="num">${formatPercent(story.semanticCoveragePercent)}</td>
</tr>`
    )
    .join('\n');
  const variantRows = [...coverage.variants]
    .sort(
      (left, right) =>
        Number(right.status === 'executed') -
          Number(left.status === 'executed') || left.id.localeCompare(right.id)
    )
    .map(
      (variant) => `<tr>
<th scope="row"><code>${escapeHtml(variant.id)}</code><span>${escapeHtml(variant.variantName)}</span></th>
<td><code>${escapeHtml(variant.storyId)}</code></td>
<td><span class="status ${variant.status === 'executed' ? 'covered' : variant.status}">${variant.status === 'executed' ? 'Passed' : variant.status === 'failed' ? 'Failed' : variant.status === 'planned' ? 'Planned' : 'Not run'}</span></td>
<td><span class="status ${variant.verification === 'asserted' ? 'covered' : variant.status}">${variant.verification === 'asserted' ? `Asserted (${variant.assertionCount})` : variant.verification === 'capture-only' ? 'Capture only' : variant.verification === 'failed' ? 'Capture failed' : variant.verification === 'planned-asserted' ? `Assertions planned (${variant.assertionCount})` : variant.verification === 'planned-capture' ? 'Capture planned' : 'Not run'}</span></td>
</tr>`
    )
    .join('\n');
  const componentRows = [...coverage.components]
    .sort(
      (left, right) =>
        Number(right.classification === 'covered') -
          Number(left.classification === 'covered') ||
        left.componentId.localeCompare(right.componentId)
    )
    .map((component) => {
      const status = statusLabel(component);
      return `<tr>
<th scope="row"><code>${escapeHtml(component.componentId)}</code></th>
<td><span class="status ${status.className}">${escapeHtml(status.label)}</span></td>
<td>${escapeHtml(status.evidence)}</td>
</tr>`;
    })
    .join('\n');
  const visualStateRows = [...coverage.visualStates]
    .sort(
      (left, right) =>
        Number(right.status === 'covered') -
          Number(left.status === 'covered') || left.id.localeCompare(right.id)
    )
    .map(
      (state) => `<tr>
<th scope="row"><code>${escapeHtml(state.id)}</code><span>${escapeHtml(state.label)}</span></th>
<td><span class="status ${state.status}">${state.status === 'covered' ? 'Covered' : state.status === 'failed' ? 'Failed' : state.status === 'planned' ? 'Planned' : 'Missed'}</span></td>
<td>${escapeHtml(state.status === 'covered' ? 'evidenced' : state.status === 'failed' ? 'capture failed' : state.status === 'planned' ? 'capture planned' : (state.gapType?.replace(/-/g, ' ') ?? 'unclassified'))}</td>
<td>${escapeHtml(state.evidence.join(', ') || (state.status === 'failed' ? 'A mapped capture or assertion failed' : state.status === 'planned' ? 'Runtime evidence not collected' : [state.gapType?.replace(/-/g, ' '), state.description].filter(Boolean).join(': ') || 'No executing variant declares this state'))}</td>
</tr>`
    )
    .join('\n');
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Visual story coverage</title>
<style>
:root{--paper:#f2efe7;--ink:#17231f;--accent:#bd4d2d;--accent-soft:#ead7ca;--mute:#596860;--line:#c9c5b9;--covered:#176145;--missed:#963b2d;--panel:#e8e4da;--font:"Avenir Next","Segoe UI",sans-serif;--mono:"SFMono-Regular","Cascadia Code","Roboto Mono",monospace}
*{box-sizing:border-box}
html{background:var(--paper)}
body{min-height:100dvh;margin:0;background:var(--paper);color:var(--ink);font-family:var(--font);font-size:15px;line-height:1.45}
header{padding:clamp(24px,5vw,64px) clamp(16px,4vw,56px) 24px;border-bottom:1px solid var(--ink)}
header h1{max-width:18ch;margin:0;font-size:clamp(30px,5vw,64px);font-weight:650;letter-spacing:-.04em;line-height:.98}
header p{max-width:68ch;margin:16px 0 0;color:var(--mute)}
main{width:100%;padding:0 clamp(16px,4vw,56px) 64px}
.baseline{border-bottom:1px solid var(--ink)}
.coverage-row{display:grid;grid-template-columns:minmax(180px,1.1fr) minmax(180px,3fr) minmax(72px,.5fr);gap:8px 24px;align-items:center;padding:24px 0;border-top:1px solid var(--line)}
.coverage-row:first-child{border-top:0}
.coverage-name h2{margin:0;font-size:18px}
.coverage-name p{margin:3px 0 0;color:var(--mute);font-family:var(--mono);font-size:12px;font-variant-numeric:tabular-nums}
.coverage-row>strong{text-align:right;font-family:var(--mono);font-size:24px;font-variant-numeric:tabular-nums}
.coverage-track{height:12px;background:var(--panel);border:1px solid var(--line)}
.coverage-track span{display:block;height:100%;background:var(--accent)}
.coverage-note{grid-column:2/4;margin:0;color:var(--mute);font-size:13px;font-style:italic}
.catalog-note{margin:16px 0 40px;color:var(--mute);font-size:13px}
.gap-summary{display:grid;grid-template-columns:repeat(5,minmax(0,1fr));gap:1px;margin:28px 0 40px;background:var(--line);border:1px solid var(--line)}
.gap-summary div{padding:18px;background:var(--paper)}.gap-summary dt{color:var(--mute);font-size:12px}.gap-summary dd{margin:4px 0 0;font-family:var(--mono);font-size:24px;font-weight:700}
.inventory{padding-top:40px}
.inventory>h2{margin:0 0 8px;font-size:24px}
.inventory>p{max-width:72ch;margin:0 0 24px;color:var(--mute)}
details{border-top:1px solid var(--ink)}
details:last-child{border-bottom:1px solid var(--ink)}
summary{display:flex;justify-content:space-between;gap:16px;padding:16px 0;cursor:pointer;font-size:18px;font-weight:650}
summary:focus-visible{outline:2px solid var(--accent);outline-offset:4px}
summary span{color:var(--mute);font-family:var(--mono);font-size:12px;font-weight:500}
.table-wrap{overflow:auto;padding-bottom:16px}
table{width:100%;border-collapse:collapse;font-size:13px;font-variant-numeric:tabular-nums}
th,td{padding:9px 12px;text-align:left;vertical-align:top}
thead th{border-bottom:1px solid var(--ink);color:var(--mute);font-size:11px;font-weight:650;letter-spacing:.05em;text-transform:uppercase}
tbody tr+tr>*{border-top:1px solid var(--line)}
tbody th{font-weight:500}
tbody th span{display:block;margin-top:2px;color:var(--mute);font-family:var(--font);font-size:12px}
.num{text-align:right}
code{font-family:var(--mono);font-size:.92em}
.status{font-weight:650}
.status.covered{color:var(--covered)}
.status.missed,.status.failed{color:var(--missed)}
.status.planned{color:var(--mute)}
@media(max-width:720px){.coverage-row{grid-template-columns:1fr auto}.coverage-track{grid-column:1/3}.coverage-note{grid-column:1/3}.coverage-row>strong{grid-column:2;grid-row:1}.gap-summary{grid-template-columns:repeat(2,minmax(0,1fr))}.table-wrap{margin-inline:-16px;padding-inline:16px}}
@media(prefers-reduced-motion:reduce){*{scroll-behavior:auto!important}}
</style>
</head>
<body>
<header><h1>Visual Stories Coverage</h1><p>${coverage.mode === 'captured' ? 'Runtime coverage for the current visual suite.' : 'Planned catalog inventory without runtime evidence.'} Percentages are informational; the detailed inventory shows exactly what is covered and what remains.</p></header>
<main>
<section class="baseline" aria-label="Coverage baseline">
${coverageMetricHtml('Story coverage', coverage.coveredStories, coverage.totalStories, coverage.storyCoveragePercent, 'A story is covered when at least one of its variants runs in this visual suite.')}
${coverageMetricHtml('Variant execution', coverage.executedVariants, coverage.totalVariants, coverage.variantExecutionPercent, 'The share of in-scope variants that completed without capture or assertion failures.', 'passed')}
${coverageMetricHtml('Semantic variant coverage', coverage.verifiedVariants, coverage.totalVariants, coverage.semanticVariantCoveragePercent, 'The share of variants with explicit assertions. Capture-only variants are inventory, not behavioral proof.', 'asserted')}
${coverageMetricHtml('Visual state coverage', coverage.coveredVisualStates, coverage.totalVisualStates, coverage.visualStateCoveragePercent, 'The share of explicitly declared UI states exercised by an executed variant. Missing states remain visible as concrete work.')}
${coverageMetricHtml('Component coverage', coverage.coveredComponents, coverage.totalComponents, coverage.componentCoveragePercent, 'A static estimate of components reachable from covered stories; conditional branches may be counted even when they do not render.')}
</section>
<p class="catalog-note">Story catalog: ${coverage.registeredStoryFiles}/${coverage.totalStoryFiles} files registered. A missing registration fails the coverage test.</p>
<dl class="gap-summary" aria-label="Open visual-state gaps">
<div><dt>Missing story</dt><dd>${gaps.missingStory}</dd></div>
<div><dt>Integration-only</dt><dd>${gaps.integrationOnly}</dd></div>
<div><dt>Product limitation</dt><dd>${gaps.productLimitation}</dd></div>
<div><dt>Visual baseline required</dt><dd>${gaps.visualBaselineRequired}</dd></div>
<div><dt>Unclassified</dt><dd>${gaps.unclassified}</dd></div>
</dl>
<section class="inventory">
<h2>Coverage detail</h2>
<p>Each inventory uses explicit Covered, Executed, or Missed labels. Component evidence names the stories that make a component statically reachable.</p>
<details open><summary>Stories <span>${coverage.coveredStories} captured · ${failedStories} failed · ${plannedStories} planned · ${coverage.totalStories - coverage.coveredStories - failedStories - plannedStories} missed</span></summary><div class="table-wrap"><table><thead><tr><th scope="col">Story</th><th scope="col">Status</th><th scope="col" class="num">Passed</th><th scope="col" class="num">Failed</th><th scope="col" class="num">Asserted</th><th scope="col" class="num">Semantic coverage</th></tr></thead><tbody>${storyRows}</tbody></table></div></details>
<details><summary>Variants <span>${coverage.executedVariants} passed · ${coverage.failedVariants} failed · ${plannedVariants} planned · ${coverage.verifiedVariants} asserted</span></summary><div class="table-wrap"><table><thead><tr><th scope="col">Variant</th><th scope="col">Story</th><th scope="col">Execution</th><th scope="col">Semantic evidence</th></tr></thead><tbody>${variantRows}</tbody></table></div></details>
<details><summary>Visual states <span>${coverage.coveredVisualStates} covered · ${coverage.failedVisualStates} failed · ${plannedVisualStates} planned · ${coverage.totalVisualStates - coverage.coveredVisualStates - coverage.failedVisualStates - plannedVisualStates} missed</span></summary><div class="table-wrap"><table><thead><tr><th scope="col">State</th><th scope="col">Status</th><th scope="col">Disposition</th><th scope="col">Variant evidence or gap</th></tr></thead><tbody>${visualStateRows}</tbody></table></div></details>
<details><summary>Components <span>${coverage.coveredComponents} covered · ${coverage.totalComponents - coverage.coveredComponents} missed</span></summary><div class="table-wrap"><table><thead><tr><th scope="col">Component</th><th scope="col">Status</th><th scope="col">Story evidence</th></tr></thead><tbody>${componentRows}</tbody></table></div></details>
</section>
</main>
</body>
</html>`;
}
