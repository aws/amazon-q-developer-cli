import * as fs from 'node:fs';
import * as path from 'node:path';
import ts from 'typescript';
import type { StorybookDefinition } from './contracts.js';

export type VisualComponentClassification =
  | 'covered'
  | 'catalog-only'
  | 'uncovered';

export interface VisualStoryCoverage {
  storyId: string;
  storyName: string;
  sourcePath: string;
  status: 'covered' | 'missed';
  executedVariants: number;
  totalVariants: number;
  coveragePercent: number;
}

export interface VisualVariantCoverage {
  id: string;
  storyId: string;
  storyName: string;
  variantId: string;
  variantName: string;
  status: 'executed' | 'missed';
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
  totalStories: number;
  coveredStories: number;
  totalVariants: number;
  executedVariants: number;
  totalStoryFiles: number;
  registeredStoryFiles: number;
  unregisteredStoryFiles: readonly string[];
  totalComponents: number;
  coveredComponents: number;
  catalogOnlyComponents: number;
  uncoveredComponents: number;
  storyCoveragePercent: number;
  variantExecutionPercent: number;
  componentCoveragePercent: number;
  stories: readonly VisualStoryCoverage[];
  variants: readonly VisualVariantCoverage[];
  components: readonly VisualComponentCoverage[];
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

function certificationMatches(
  variant: StorybookDefinition['variants'][number],
  suite?: string
): boolean {
  const certification = variant.parameters.certification;
  return (
    certification !== undefined &&
    (suite === undefined || certification.suite === suite)
  );
}

export function collectVisualCoverage(
  sourceRoot: string,
  stories: readonly StorybookDefinition[],
  suite?: string
): VisualCoverage {
  const normalizedRoot = path.resolve(sourceRoot);
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
      if (
        story.variants.some((variant) => certificationMatches(variant, suite))
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
  const storyCoverage = stories
    .map((story): VisualStoryCoverage => {
      const executedVariants = story.variants.filter((variant) =>
        certificationMatches(variant, suite)
      ).length;
      return {
        storyId: story.id,
        storyName: story.name,
        sourcePath: story.sourcePath,
        status: executedVariants > 0 ? 'covered' : 'missed',
        executedVariants,
        totalVariants: story.variants.length,
        coveragePercent: percent(executedVariants, story.variants.length),
      };
    })
    .sort((left, right) => left.storyId.localeCompare(right.storyId));
  const variantCoverage = stories
    .flatMap((story) =>
      story.variants.map(
        (variant): VisualVariantCoverage => ({
          id: `${story.id}/${variant.id}`,
          storyId: story.id,
          storyName: story.name,
          variantId: variant.id,
          variantName: variant.name,
          status: certificationMatches(variant, suite) ? 'executed' : 'missed',
        })
      )
    )
    .sort((left, right) => left.id.localeCompare(right.id));
  const coveredStories = storyCoverage.filter(
    (story) => story.status === 'covered'
  ).length;
  const executedVariants = variantCoverage.filter(
    (variant) => variant.status === 'executed'
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
    totalStories: storyCoverage.length,
    coveredStories,
    totalVariants: variantCoverage.length,
    executedVariants,
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
    componentCoveragePercent: percent(
      coveredComponents,
      componentCoverage.length
    ),
    stories: storyCoverage,
    variants: variantCoverage,
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

function coverageSummaryMarkdown(coverage: VisualCoverage): string {
  return `# Visual Stories Coverage

| Metric | Coverage | Covered | Missed | Total |
| --- | ---: | ---: | ---: | ---: |
| Story coverage | **${formatPercent(coverage.storyCoveragePercent)}** | ${coverage.coveredStories} | ${coverage.totalStories - coverage.coveredStories} | ${coverage.totalStories} |
| Variant execution | **${formatPercent(coverage.variantExecutionPercent)}** | ${coverage.executedVariants} | ${coverage.totalVariants - coverage.executedVariants} | ${coverage.totalVariants} |
| Component coverage | **${formatPercent(coverage.componentCoveragePercent)}** | ${coverage.coveredComponents} | ${coverage.totalComponents - coverage.coveredComponents} | ${coverage.totalComponents} |

*Story coverage: a story is covered when at least one of its variants runs in this visual suite.*

*Variant execution: the share of registered variants selected and run by this visual suite.*

*Component coverage: a static estimate of components reachable from covered stories; conditional branches may be counted even when they do not render in a captured frame.*

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
        `- \`${story.storyId}\` - ${story.executedVariants}/${story.totalVariants} variants executed`
    );
  const missedStories = coverage.stories
    .filter((story) => story.status === 'missed')
    .map(
      (story) =>
        `- \`${story.storyId}\` - 0/${story.totalVariants} variants executed`
    );
  const executedVariants = coverage.variants
    .filter((variant) => variant.status === 'executed')
    .map((variant) => `- \`${variant.id}\``);
  const missedVariants = coverage.variants
    .filter((variant) => variant.status === 'missed')
    .map((variant) => `- \`${variant.id}\``);
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

## Variant Detail

### Executed (${executedVariants.length})

${markdownList(executedVariants)}

### Missed (${missedVariants.length})

${markdownList(missedVariants)}

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
  const missed = total - covered;
  return `<article class="coverage-row">
  <div class="coverage-name"><h2>${escapeHtml(label)}</h2><p>${covered} ${coveredTerm} · ${missed} missed · ${total} total</p></div>
  <div class="coverage-track" role="img" aria-label="${escapeHtml(`${label}: ${formatPercent(coveragePercent)}, ${covered} ${coveredTerm}, ${missed} missed, ${total} total`)}"><span style="width:${coveragePercent.toFixed(1)}%"></span></div>
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
<td><span class="status ${story.status}">${story.status === 'covered' ? 'Covered' : 'Missed'}</span></td>
<td class="num">${story.executedVariants}/${story.totalVariants}</td>
<td class="num">${formatPercent(story.coveragePercent)}</td>
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
<td><span class="status ${variant.status === 'executed' ? 'covered' : 'missed'}">${variant.status === 'executed' ? 'Executed' : 'Missed'}</span></td>
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
.status.missed{color:var(--missed)}
@media(max-width:720px){.coverage-row{grid-template-columns:1fr auto}.coverage-track{grid-column:1/3}.coverage-note{grid-column:1/3}.coverage-row>strong{grid-column:2;grid-row:1}.table-wrap{margin-inline:-16px;padding-inline:16px}}
@media(prefers-reduced-motion:reduce){*{scroll-behavior:auto!important}}
</style>
</head>
<body>
<header><h1>Visual Stories Coverage</h1><p>Baseline coverage for the current visual suite. Percentages are informational; the detailed inventory shows exactly what is covered and what remains.</p></header>
<main>
<section class="baseline" aria-label="Coverage baseline">
${coverageMetricHtml('Story coverage', coverage.coveredStories, coverage.totalStories, coverage.storyCoveragePercent, 'A story is covered when at least one of its variants runs in this visual suite.')}
${coverageMetricHtml('Variant execution', coverage.executedVariants, coverage.totalVariants, coverage.variantExecutionPercent, 'The share of registered variants selected and run by this visual suite.', 'executed')}
${coverageMetricHtml('Component coverage', coverage.coveredComponents, coverage.totalComponents, coverage.componentCoveragePercent, 'A static estimate of components reachable from covered stories; conditional branches may be counted even when they do not render.')}
</section>
<p class="catalog-note">Story catalog: ${coverage.registeredStoryFiles}/${coverage.totalStoryFiles} files registered. A missing registration fails the coverage test.</p>
<section class="inventory">
<h2>Coverage detail</h2>
<p>Each inventory uses explicit Covered, Executed, or Missed labels. Component evidence names the stories that make a component statically reachable.</p>
<details open><summary>Stories <span>${coverage.coveredStories} covered · ${coverage.totalStories - coverage.coveredStories} missed</span></summary><div class="table-wrap"><table><thead><tr><th scope="col">Story</th><th scope="col">Status</th><th scope="col" class="num">Variants</th><th scope="col" class="num">Coverage</th></tr></thead><tbody>${storyRows}</tbody></table></div></details>
<details><summary>Variants <span>${coverage.executedVariants} executed · ${coverage.totalVariants - coverage.executedVariants} missed</span></summary><div class="table-wrap"><table><thead><tr><th scope="col">Variant</th><th scope="col">Story</th><th scope="col">Status</th></tr></thead><tbody>${variantRows}</tbody></table></div></details>
<details><summary>Components <span>${coverage.coveredComponents} covered · ${coverage.totalComponents - coverage.coveredComponents} missed</span></summary><div class="table-wrap"><table><thead><tr><th scope="col">Component</th><th scope="col">Status</th><th scope="col">Story evidence</th></tr></thead><tbody>${componentRows}</tbody></table></div></details>
</section>
</main>
</body>
</html>`;
}
