import { describe, expect, it } from 'bun:test';
import fs from 'node:fs';
import path from 'node:path';
import * as ts from 'typescript';

const sourceRoot = path.resolve(import.meta.dir, '../..');
const componentsRoot = path.join(sourceRoot, 'components');
const appContainerPath = 'components/layout/AppContainer.tsx';

// Shared components that intentionally vary data or one-sided visibility.
// Every addition is an explicit architecture decision; stale entries fail.
const allowedModeDataReaders = new Set([
  'components/chat/prompt-bar/PromptInput.tsx',
  'components/layout/lite/LiteLayout.tsx',
  'components/ui/CommandMenu.tsx',
  'components/ui/ToolUseMessage.tsx',
  'components/ui/command-menu-utils.ts',
  'components/ui/menu/VerbosityPreview.tsx',
  'components/ui/menu/VerbosityTruncationEditor.tsx',
]);

function sourceFiles(directory: string): string[] {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const filePath = path.join(directory, entry.name);
    if (entry.isDirectory()) return sourceFiles(filePath);
    return /\.[cm]?tsx?$/.test(entry.name) ? [filePath] : [];
  });
}

function relativePath(filePath: string): string {
  return path.relative(sourceRoot, filePath).split(path.sep).join('/');
}

function parseSource(
  fileName: string,
  text = fs.readFileSync(fileName, 'utf8')
): ts.SourceFile {
  return ts.createSourceFile(
    fileName,
    text,
    ts.ScriptTarget.Latest,
    true,
    fileName.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS
  );
}

function isProductionSource(filePath: string): boolean {
  return (
    !filePath.includes('/__tests__/') &&
    !/\.(?:test|stories)\.[cm]?tsx?$/.test(filePath)
  );
}

function bindingReadsUiMode(node: ts.BindingElement): boolean {
  if (!node.propertyName) {
    return ts.isIdentifier(node.name) && node.name.text === 'uiMode';
  }
  if (
    (ts.isIdentifier(node.propertyName) ||
      ts.isStringLiteralLike(node.propertyName)) &&
    node.propertyName.text === 'uiMode'
  ) {
    return true;
  }
  return (
    ts.isComputedPropertyName(node.propertyName) &&
    ts.isStringLiteralLike(node.propertyName.expression) &&
    node.propertyName.expression.text === 'uiMode'
  );
}

// This is deliberately lexical: dynamic keys and helper-returned aliases cannot
// be classified as uiMode reads without interpreting arbitrary program flow.
function readsUiMode(source: ts.SourceFile): boolean {
  let found = false;
  const visit = (node: ts.Node): void => {
    if (
      (ts.isPropertyAccessExpression(node) && node.name.text === 'uiMode') ||
      (ts.isElementAccessExpression(node) &&
        !!node.argumentExpression &&
        ts.isStringLiteralLike(node.argumentExpression) &&
        node.argumentExpression.text === 'uiMode') ||
      (ts.isBindingElement(node) && bindingReadsUiMode(node)) ||
      (ts.isIdentifier(node) &&
        node.text === 'uiMode' &&
        ts.isParameter(node.parent))
    ) {
      found = true;
    }
    if (!found) ts.forEachChild(node, visit);
  };
  visit(source);
  return found;
}

describe('UI mode surface boundary', () => {
  it('keeps direct production uiMode reads centralized or explicit', () => {
    const files = sourceFiles(componentsRoot).filter((filePath) =>
      isProductionSource(relativePath(filePath))
    );
    const readers = new Set(
      files
        .filter((filePath) => readsUiMode(parseSource(filePath)))
        .map(relativePath)
    );

    expect(readers.has(appContainerPath)).toBe(true);
    expect(
      [...allowedModeDataReaders].filter((file) => !readers.has(file))
    ).toEqual([]);
    expect(
      [...readers].filter(
        (file) => file !== appContainerPath && !allowedModeDataReaders.has(file)
      )
    ).toEqual([]);
  });

  it('recognizes each supported direct read form without matching names alone', () => {
    const directReads = [
      `if (state.uiMode === 'lite') renderLite();`,
      `switch (state['uiMode']) { case 'lite': renderLite(); }`,
      `const { uiMode: variant } = state;`,
      `const { ['uiMode']: variant } = state;`,
      `const Component = ({ uiMode: variant }) => variant;`,
    ];

    for (const [index, source] of directReads.entries()) {
      expect(readsUiMode(parseSource(`read-${index}.tsx`, source))).toBe(true);
    }

    expect(
      readsUiMode(
        parseSource(
          'names.tsx',
          `
            const uiModeLabel = 'Display mode';
            const value = state.mode;
          `
        )
      )
    ).toBe(false);
  });
});
