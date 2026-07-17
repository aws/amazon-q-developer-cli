import { describe, expect, it } from 'bun:test';
import fs from 'node:fs';
import path from 'node:path';
import * as ts from 'typescript';
import {
  createInitialKasSubagentRoutingState,
  createKasSubagentRoutingActions,
} from '../../stores/kas-subagent-routing';

const sourceRoot = path.resolve(import.meta.dir, '../..');
const acpClientRoot = path.join(sourceRoot, 'acp-client');
// This is deliberately lexical: a source guard cannot infer whether an
// arbitrary field is display state, so it enforces cache-shaped names only.
const cacheNamedField = /(?:^_*(?:cached|cache(?:$|[A-Z_]))|Cache(?:$|[A-Z_]))/;

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

function isProductionSource(filePath: string): boolean {
  return (
    !filePath.includes('/__tests__/') &&
    !/\.(?:test|stories)\.[cm]?tsx?$/.test(filePath)
  );
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
    filePath.endsWith('x') ? ts.ScriptKind.TSX : ts.ScriptKind.TS
  );
}

function fieldName(name: ts.PropertyName | ts.BindingName): string | null {
  if (
    ts.isComputedPropertyName(name) &&
    ts.isStringLiteralLike(name.expression)
  ) {
    return name.expression.text;
  }
  if (
    !ts.isIdentifier(name) &&
    !ts.isPrivateIdentifier(name) &&
    !ts.isStringLiteralLike(name)
  ) {
    return null;
  }
  return name.text.replace(/^#/, '');
}

function isParameterProperty(parameter: ts.ParameterDeclaration): boolean {
  return (
    parameter.modifiers?.some((modifier) =>
      [
        ts.SyntaxKind.PublicKeyword,
        ts.SyntaxKind.ProtectedKeyword,
        ts.SyntaxKind.PrivateKeyword,
        ts.SyntaxKind.ReadonlyKeyword,
      ].includes(modifier.kind)
    ) ?? false
  );
}

function declaredCacheNamedFields(
  relativePath: string,
  source: ts.SourceFile
): string[] {
  const fields: string[] = [];

  const visit = (node: ts.Node): void => {
    if (ts.isClassDeclaration(node) || ts.isClassExpression(node)) {
      for (const member of node.members) {
        const declarations: Array<
          ts.PropertyDeclaration | ts.ParameterDeclaration
        > = [];
        if (ts.isPropertyDeclaration(member)) {
          if (
            !member.modifiers?.some(
              (modifier) => modifier.kind === ts.SyntaxKind.StaticKeyword
            )
          ) {
            declarations.push(member);
          }
        } else if (ts.isConstructorDeclaration(member)) {
          declarations.push(...member.parameters.filter(isParameterProperty));
        }

        for (const declaration of declarations) {
          const name = fieldName(declaration.name);
          if (!name || !cacheNamedField.test(name)) continue;
          fields.push(
            `${relativePath}:${node.name?.text ?? '<anonymous>'}.${name}`
          );
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);

  return fields.sort();
}

function productionAcpSources(): Array<readonly [string, string]> {
  return sourceFiles(acpClientRoot)
    .map((filePath) => [relativePath(filePath), filePath] as const)
    .filter(([relativeFile]) => isProductionSource(relativeFile));
}

function storeOwnedKasRoutingMembers(): Set<string> {
  const state = createInitialKasSubagentRoutingState();
  const actions = createKasSubagentRoutingActions(() => state);
  return new Set([...Object.keys(state), ...Object.keys(actions)]);
}

function declaredStoreOwnedRoutingMembers(
  relativePath: string,
  source: ts.SourceFile,
  storeMembers: ReadonlySet<string>
): string[] {
  const members: string[] = [];

  const visit = (node: ts.Node): void => {
    if (ts.isClassDeclaration(node) || ts.isClassExpression(node)) {
      for (const member of node.members) {
        const names: Array<ts.PropertyName | ts.BindingName> = [];
        if (
          ts.isPropertyDeclaration(member) ||
          ts.isMethodDeclaration(member) ||
          ts.isGetAccessorDeclaration(member) ||
          ts.isSetAccessorDeclaration(member)
        ) {
          names.push(member.name);
        } else if (ts.isConstructorDeclaration(member)) {
          names.push(
            ...member.parameters
              .filter(isParameterProperty)
              .map((parameter) => parameter.name)
          );
        }
        for (const nameNode of names) {
          const name = fieldName(nameNode);
          if (!name || !storeMembers.has(name)) continue;
          members.push(
            `${relativePath}:${node.name?.text ?? '<anonymous>'}.${name}`
          );
        }
      }
    }
    ts.forEachChild(node, visit);
  };
  visit(source);

  return members.sort();
}

describe('ACP client state boundary', () => {
  it('keeps cache-named instance state out of ACP transport sources', () => {
    const files = productionAcpSources();
    expect(files.length).toBeGreaterThan(0);

    const violations = files.flatMap(([relativeFile, filePath]) =>
      declaredCacheNamedFields(relativeFile, parseSource(filePath))
    );

    expect(violations).toEqual([]);
  });

  it('detects cache-named properties and parameter properties', () => {
    const fixture = parseSource(
      'fixture.ts',
      `
        class FutureClient {
          private cache = {};
          private cachedFoo = [];
          #barCache = new Map();
          private promptsCache = [];
          private ['computedCache'] = [];
          static sharedCache = [];
          constructor(readonly cachedTools: string[]) {}
        }
        class Helper {
          private latestTools = [];
          private prompts = [];
        }
      `
    );
    expect(declaredCacheNamedFields('fixture.ts', fixture)).toEqual([
      'fixture.ts:FutureClient.barCache',
      'fixture.ts:FutureClient.cache',
      'fixture.ts:FutureClient.cachedFoo',
      'fixture.ts:FutureClient.cachedTools',
      'fixture.ts:FutureClient.computedCache',
      'fixture.ts:FutureClient.promptsCache',
    ]);
  });

  it('keeps store-owned KAS routing state and actions off transport clients', () => {
    const storeMembers = storeOwnedKasRoutingMembers();
    expect(storeMembers.size).toBeGreaterThan(0);

    const violations = productionAcpSources().flatMap(
      ([relativeFile, filePath]) =>
        declaredStoreOwnedRoutingMembers(
          relativeFile,
          parseSource(filePath),
          storeMembers
        )
    );

    expect(violations).toEqual([]);
  });
});
