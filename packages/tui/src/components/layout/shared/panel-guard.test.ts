import { describe, expect, it } from 'bun:test';
import fs from 'node:fs';
import path from 'node:path';
import ts from 'typescript';
import {
  BACKEND_PANEL_COMPONENT_NAMES,
  BACKEND_PANEL_STATE_KEYS,
} from './BackendPanels.js';

const sourceRoot = path.resolve(import.meta.dir, '../../..');
const backendPanelsPath = path.join(import.meta.dir, 'BackendPanels.tsx');
const layoutPaths = [
  path.join(sourceRoot, 'components/layout/InlineLayout.tsx'),
  path.join(sourceRoot, 'components/layout/lite/LiteLayout.tsx'),
];

function parse(filePath: string): ts.SourceFile {
  return ts.createSourceFile(
    filePath,
    fs.readFileSync(filePath, 'utf8'),
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TSX
  );
}

function importedPanelNames(source: ts.SourceFile): string[] {
  const names: string[] = [];
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement)) continue;
    const moduleName = (statement.moduleSpecifier as ts.StringLiteral).text;
    const clause = statement.importClause?.namedBindings;
    if (!clause || !ts.isNamedImports(clause)) continue;
    for (const element of clause.elements) {
      const imported = element.propertyName?.text ?? element.name.text;
      if (
        moduleName !== './shared/BackendPanels.js' &&
        moduleName !== '../shared/BackendPanels.js' &&
        (BACKEND_PANEL_COMPONENT_NAMES as readonly string[]).includes(imported)
      ) {
        names.push(imported);
      }
    }
  }
  return names;
}

function identifiers(source: ts.SourceFile): string[] {
  const names: string[] = [];
  const visit = (node: ts.Node): void => {
    if (ts.isIdentifier(node)) names.push(node.text);
    ts.forEachChild(node, visit);
  };
  visit(source);
  return names;
}

describe('backend panel ownership guard', () => {
  it('keeps panel state and component mounts in the shared cluster', () => {
    const backendPanels = parse(backendPanelsPath);
    const backendIdentifiers = new Set(identifiers(backendPanels));

    for (const stateKey of BACKEND_PANEL_STATE_KEYS) {
      expect(backendIdentifiers.has(stateKey), stateKey).toBe(true);
    }
    for (const componentName of BACKEND_PANEL_COMPONENT_NAMES) {
      expect(backendIdentifiers.has(componentName), componentName).toBe(true);
    }

    for (const layoutPath of layoutPaths) {
      const layout = parse(layoutPath);
      const layoutText = layout.getFullText();
      expect(importedPanelNames(layout), layoutPath).toEqual([]);
      expect(
        (layoutText.match(/<BackendPanels\b/g) ?? []).length,
        layoutPath
      ).toBe(1);
      expect(
        (layoutText.match(/\buseBackendPanelVisibility\s*\(/g) ?? []).length,
        layoutPath
      ).toBe(1);
      for (const stateKey of BACKEND_PANEL_STATE_KEYS) {
        expect(
          identifiers(layout).includes(stateKey),
          `${layoutPath} reads ${stateKey} directly`
        ).toBe(false);
      }
    }
  });

  it('catches a copied panel import', () => {
    const fixture = ts.createSourceFile(
      'fixture.tsx',
      "import { HelpPanel } from '../ui/HelpPanel.js';",
      ts.ScriptTarget.Latest,
      true,
      ts.ScriptKind.TSX
    );
    expect(importedPanelNames(fixture)).toEqual(['HelpPanel']);
  });
});
