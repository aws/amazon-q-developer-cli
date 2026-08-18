import { describe, expect, it } from 'bun:test';
import fs from 'node:fs';
import path from 'node:path';
import * as ts from 'typescript';
import { KNOWN_TOOL_NAMES } from '../tool-capabilities.js';

const sourceRoot = path.resolve(import.meta.dir, '../..');

const ALLOWED_CANONICAL_NAME_COLLECTIONS = new Set([
  // Built-in ids and labels are the source vocabulary consumed by the
  // capability registry, not a second UI dispatch list.
  'types/tool-status.ts:TOOL_LABELS',
  // Argument values and field priorities, not top-level tool wire names.
  'utils/spec-artifact-path.ts:WRITE_COMMAND_VALUES',
  'utils/collapsed-tool-view.ts:PRIMARY_ARG_KEYS',
  // Settings and preview vocabularies that reuse words such as knowledge,
  // task, and subagent without classifying tool calls.
  'utils/kas-settings.ts:GATED_FEATURES',
  'utils/kas-settings.ts:boolMappings',
  'components/ui/CommandMenu.tsx:VERBOSITY_PREVIEW_KEYS',
  // Slash-command admission policy, not tool call classification.
  'commands/command-registry.ts:ARGUMENT_SENSITIVE_COMMANDS',
  // Renderer ids are typed by ScrollbackToolRenderer and resolved only after
  // the capability registry has classified the tool.
  'components/ui/ToolUseMessage.tsx:labels',
  // Operation labels inside one already-classified tool renderer.
  'components/chat/tools/Code.tsx:VERBOSE_OPS',
  'components/chat/tools/Code.tsx:OP_LABELS',
  'components/chat/tools/SessionTool.tsx:ACTION_LABELS',
  // Deterministic verbosity-preview fixture selection.
  'lite/render.ts:CANDIDATES',
  'lite/render.ts:PREVIEW_SETS',
  // Semantic footer ids, not tool call names.
  'components/layout/status-line/segments.ts:STATUS_SEGMENT_IDS',
  'components/layout/status-line/segments.ts:ID_SET',
  // User-visible tangent names that would collide with navigation commands.
  'commands/kas-handlers/tangent.ts:RESERVED_NAMES',
  // Common workspace-directory basenames used to disambiguate list labels;
  // reuses the word "code" as a folder name, not the code tool.
  'utils/session-dashboard.ts:generic',
]);

function sourceFiles(directory: string): string[] {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const filePath = path.join(directory, entry.name);
    if (entry.isDirectory()) return sourceFiles(filePath);
    return /\.[cm]?tsx?$/.test(entry.name) ? [filePath] : [];
  });
}

function parseSource(
  filePath: string,
  text = fs.readFileSync(filePath, 'utf8')
) {
  return ts.createSourceFile(
    filePath,
    text,
    ts.ScriptTarget.Latest,
    true,
    filePath.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS
  );
}

function variableInitializers(
  source: ts.SourceFile
): Map<string, ts.Expression> {
  const initializers = new Map<string, ts.Expression>();
  const visit = (node: ts.Node): void => {
    if (
      ts.isVariableDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      node.initializer
    ) {
      initializers.set(node.name.text, node.initializer);
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return initializers;
}

function collectionStrings(
  expression: ts.Expression,
  initializers: ReadonlyMap<string, ts.Expression>,
  seen = new Set<string>()
): string[] | null {
  while (
    ts.isParenthesizedExpression(expression) ||
    ts.isAsExpression(expression) ||
    ts.isSatisfiesExpression(expression)
  ) {
    expression = expression.expression;
  }

  if (ts.isArrayLiteralExpression(expression)) {
    return expression.elements.flatMap((element) => {
      const nested = ts.isSpreadElement(element)
        ? collectionStrings(element.expression, initializers, seen)
        : ts.isExpression(element)
          ? collectionStrings(element, initializers, seen)
          : null;
      return nested ?? [];
    });
  }
  if (ts.isObjectLiteralExpression(expression)) {
    return expression.properties.flatMap((property) => {
      if (
        !ts.isPropertyAssignment(property) &&
        !ts.isShorthandPropertyAssignment(property) &&
        !ts.isMethodDeclaration(property)
      ) {
        return [];
      }
      const name = property.name;
      return ts.isIdentifier(name) || ts.isStringLiteralLike(name)
        ? [name.text]
        : [];
    });
  }
  if (
    ts.isNewExpression(expression) &&
    ['Set', 'Map'].includes(expression.expression.getText())
  ) {
    return (
      expression.arguments?.flatMap(
        (argument) => collectionStrings(argument, initializers, seen) ?? []
      ) ?? []
    );
  }
  if (
    ts.isCallExpression(expression) &&
    expression.expression.getText() === 'Object.freeze'
  ) {
    return expression.arguments.flatMap(
      (argument) => collectionStrings(argument, initializers, seen) ?? []
    );
  }
  if (ts.isStringLiteralLike(expression)) return [expression.text];
  if (ts.isIdentifier(expression)) {
    if (seen.has(expression.text)) return [];
    const initializer = initializers.get(expression.text);
    return initializer
      ? collectionStrings(
          initializer,
          initializers,
          new Set(seen).add(expression.text)
        )
      : [];
  }
  return null;
}

function isGuardedCollection(
  expression: ts.Expression,
  initializers: ReadonlyMap<string, ts.Expression>,
  seen = new Set<string>()
): boolean {
  while (
    ts.isParenthesizedExpression(expression) ||
    ts.isAsExpression(expression) ||
    ts.isSatisfiesExpression(expression)
  ) {
    expression = expression.expression;
  }
  if (ts.isArrayLiteralExpression(expression)) return true;
  if (
    ts.isNewExpression(expression) &&
    ['Set', 'Map'].includes(expression.expression.getText())
  ) {
    return true;
  }
  if (
    ts.isCallExpression(expression) &&
    expression.expression.getText() === 'Object.freeze'
  ) {
    return expression.arguments.some((argument) =>
      isGuardedCollection(argument, initializers, seen)
    );
  }
  if (ts.isIdentifier(expression)) {
    if (seen.has(expression.text)) return false;
    const initializer = initializers.get(expression.text);
    return initializer
      ? isGuardedCollection(
          initializer,
          initializers,
          new Set(seen).add(expression.text)
        )
      : false;
  }
  return false;
}

function canonicalToolCollections(source: ts.SourceFile): string[] {
  const initializers = variableInitializers(source);
  const collections: string[] = [];
  for (const [name, initializer] of initializers) {
    if (!isGuardedCollection(initializer, initializers)) continue;
    const values = collectionStrings(initializer, initializers);
    if (values?.some((value) => KNOWN_TOOL_NAMES.has(value))) {
      collections.push(name);
    }
  }
  return collections;
}

describe('canonical tool-name collection guard', () => {
  it('keeps production tool-name sets in the capability registry', () => {
    const violations: string[] = [];
    for (const filePath of sourceFiles(sourceRoot)) {
      const relativePath = path
        .relative(sourceRoot, filePath)
        .replaceAll(path.sep, '/');
      if (
        relativePath === 'types/tool-capabilities.ts' ||
        relativePath.includes('/__tests__/') ||
        relativePath.includes('/generated/') ||
        relativePath.startsWith('utils/agent-migration/') ||
        /\.(?:test|stories)\.[cm]?tsx?$/.test(relativePath)
      ) {
        continue;
      }
      for (const name of canonicalToolCollections(parseSource(filePath))) {
        const identity = `${relativePath}:${name}`;
        if (!ALLOWED_CANONICAL_NAME_COLLECTIONS.has(identity)) {
          violations.push(identity);
        }
      }
    }
    expect(violations).toEqual([]);
  });

  it('rejects copied arrays, sets, maps, and derived collections', () => {
    const source = parseSource(
      'fixture.ts',
      `
        const writeNames = ['fs_write'];
        const readNames = new Set(['fs_read']);
        const shellNames = Object.freeze(['execute_bash']);
        const rendererByName = new Map([['web_fetch', 'web']]);
        const combined = [...writeNames, ...readNames];
        const unrelated = ['alpha', 'beta'];
        const scalar = 'fs_write';
      `
    );

    expect(canonicalToolCollections(source).sort()).toEqual([
      'combined',
      'readNames',
      'rendererByName',
      'shellNames',
      'writeNames',
    ]);
  });
});
