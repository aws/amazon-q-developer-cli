/**
 * Vitest configuration -- second test runner alongside bun:test.
 *
 * Why two runners?
 * Bun's built-in V8 coverage engine cannot attribute execution that happens
 * inside React's reconciler loop. Hooks and selectors are invoked by the
 * reconciler on behalf of components, so bun:test reports 0% coverage for
 * code that is actually exercised. Vitest instruments at a different level
 * (source-level V8 coverage outside the reconciler's scheduling) and
 * correctly attributes those hits. We therefore run hook and selector tests
 * under vitest while keeping all other unit tests under bun:test.
 */
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.vitest.{ts,tsx}"],
    passWithNoTests: true,
    environment: "node",
    coverage: {
      provider: "v8",
      reportsDirectory: "./coverage/vitest",
      reporter: ["text", "lcov"],
      include: ["src/hooks/**", "src/stores/selectors.ts"],
    },
  },
});
