import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  timeout: 30000,
  use: {
    baseURL: 'http://localhost:3000',
    viewport: { width: 1600, height: 2160 },
  },
  webServer: {
    command: 'bun run pty-server.ts',
    port: 3000,
    reuseExistingServer: !process.env.CI,
    timeout: 10000,
  },
});
