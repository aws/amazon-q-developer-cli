import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    coverage: {
      enabled: true,
      thresholds: {
        lines: 68.46,
        functions: 69.88,
        statements: 68.46,
        branches: 76.18,
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