import { defineConfig } from 'vitest/config';

export default defineConfig({
  test: {
    environment: 'node',
    include: ['**/*.perf.test.{ts,tsx}'],
    // Timing numbers are only comparable when one file has the cores to itself.
    fileParallelism: false,
    testTimeout: 120_000,
  }
});
