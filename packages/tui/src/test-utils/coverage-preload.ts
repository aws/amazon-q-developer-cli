/**
 * Coverage preload -- forces bun's V8 coverage to see every production source file.
 * Without this, untested files are silently omitted from the coverage report.
 * Files that fail to import (e.g., missing dependencies) are silently skipped.
 *
 * Exclusion patterns are defined in coverage-config.json (single source of truth).
 */

import { relative } from 'node:path';
import config from '../../coverage-config.json';

const srcDir = new URL('.', import.meta.url).pathname.replace(
  /\/test-utils\/$/,
  ''
);

const excludeDirs = config.excludeDirectories.map((d) => d.name);
const excludeFiles = config.excludeFiles.map((f) => f.name);
const excludePatterns = config.excludePatterns.map((p) => p.pattern);

const globs = [new Bun.Glob('**/*.ts'), new Bun.Glob('**/*.tsx')];

for (const glob of globs) {
  for await (const filePath of glob.scan({ cwd: srcDir, absolute: true })) {
    const rel = relative(srcDir, filePath);

    if (excludeDirs.some((dir) => rel.startsWith(`${dir}/`))) continue;
    if (excludeFiles.some((file) => rel === file)) continue;
    if (excludePatterns.some((pat) => rel.includes(pat))) continue;

    try {
      await import(filePath);
    } catch {
      // Silently ignore -- many files will fail due to missing deps or side effects
    }
  }
}
