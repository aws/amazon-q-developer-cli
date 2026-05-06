import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    coverage: {
      enabled: true,
      reporter: ['text', 'lcov'],
      reportsDirectory: './coverage',
      thresholds: {
        lines: 90,
        functions: 85,
        statements: 90,
        branches: 85,
        autoUpdate: true,
      },
      exclude: [
        '**/dist/**',
        '**/test/**',
        '**/examples/**',
        'examples/**',
        'docs/**',
        'scripts/**',
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