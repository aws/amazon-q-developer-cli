import { describe, expect, it } from 'bun:test';
import fs from 'node:fs';
import path from 'node:path';
import * as ts from 'typescript';

const toolsRoot = path.resolve(import.meta.dir, '../../components/chat/tools');

function productionSources(directory: string): string[] {
  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const filePath = path.join(directory, entry.name);
    if (entry.isDirectory()) return productionSources(filePath);
    if (!/\.[cm]?tsx?$/.test(entry.name)) return [];
    if (/\.(?:test|stories)\.[cm]?tsx?$/.test(entry.name)) return [];
    return [filePath];
  });
}

function parse(filePath: string, text = fs.readFileSync(filePath, 'utf8')) {
  return ts.createSourceFile(
    filePath,
    text,
    ts.ScriptTarget.Latest,
    true,
    filePath.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS
  );
}

function globalVerbosityImports(source: ts.SourceFile): string[] {
  const imports: string[] = [];
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement)) continue;
    const moduleName = (statement.moduleSpecifier as ts.StringLiteral).text;
    const clause = statement.importClause;
    if (!clause) continue;

    if (moduleName.endsWith('/hooks/useVerbose.js')) {
      imports.push(moduleName);
      continue;
    }
    if (!moduleName.endsWith('/lite/verbose.js') || clause.isTypeOnly) continue;

    const bindings = clause.namedBindings;
    if (
      clause.name ||
      (bindings &&
        (ts.isNamespaceImport(bindings) ||
          bindings.elements.some((element) => !element.isTypeOnly)))
    ) {
      imports.push(moduleName);
    }
  }
  return imports;
}

describe('tool verbosity boundary', () => {
  it('keeps production tool renderers on their shaped context policy', () => {
    const violations = productionSources(toolsRoot).flatMap((filePath) =>
      globalVerbosityImports(parse(filePath)).map(
        (moduleName) => `${path.relative(toolsRoot, filePath)} -> ${moduleName}`
      )
    );
    expect(violations).toEqual([]);
  });

  it('rejects value imports while permitting config types', () => {
    expect(
      globalVerbosityImports(
        parse(
          'fixture.tsx',
          `
            import { useVerboseDisplay } from '../../../hooks/useVerbose.js';
            import { DEFAULT_DISPLAY } from '../../../lite/verbose.js';
            import type { VerboseDisplayConfig } from '../../../lite/verbose.js';
          `
        )
      )
    ).toEqual(['../../../hooks/useVerbose.js', '../../../lite/verbose.js']);
  });
});
