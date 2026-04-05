import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    coverage: {
      enabled: true,
      thresholds: {
        lines: 68,
        functions: 68,
        statements: 68,
        branches: 68,
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