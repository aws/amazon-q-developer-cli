/**
 * Coverage preload — forces bun's V8 coverage to see every production source file.
 * Without this, untested files are silently omitted from the coverage report.
 *
 * Exclusion patterns are defined in coverage-config.json (single source of truth).
 */

import { relative } from 'node:path';
import config from '../../coverage-config.json';

const srcDir = new URL('.', import.meta.url).pathname.replace(
  /\/test-utils\/$/,
  ''
);

const excludePatterns = config.excludePatterns.map((p) => p.pattern);

const globs = [new Bun.Glob('**/*.ts'), new Bun.Glob('**/*.tsx')];

for (const glob of globs) {
  for await (const filePath of glob.scan({ cwd: srcDir, absolute: true })) {
    const rel = relative(srcDir, filePath);
    if (excludePatterns.some((pat) => new RegExp(pat).test(rel))) continue;

    try {
      await import(filePath);
    } catch {
      // Silently ignore — many files fail due to missing deps or side effects
    }
  }
}
