import { configDefaults, defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    // Wall-clock budgets measure the runner, not the code, so they are kept out
    // of the suite that gates merges.
    exclude: [...configDefaults.exclude, '**/*.perf.test.{ts,tsx}'],
    coverage: {
      enabled: true,
      reporter: ['text', 'lcov'],
      reportsDirectory: './coverage',
      thresholds: {
        lines: 90,
        functions: 85,
        statements: 90,
        branches: 85,
        autoUpdate: false,
      },
      exclude: [
        '**/dist/**',
        '**/test/**',
        '**/examples/**',
        'examples/**',
        'docs/**',
        'scripts/**',
        // Nested workspace packages have their own scripts/ dirs; the
        // root-relative pattern above does not reach them.
        '**/scripts/**',
        'packages/testing/**',
        'packages/testing-library/**',
        '**/vitest.config.ts',
        '**/global-setup.ts',
        '**/types/props.ts',
        '**/types.ts',
        '**/terminal/terminal.ts',
        '**/word-wrap-optimized.ts',
      ],
    },
  }
});