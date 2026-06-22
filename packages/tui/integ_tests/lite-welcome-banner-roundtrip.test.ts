import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { switchToLite, switchToTui } from '../e2e_tests/lite/helpers/mode-swap';
import {
  exitLiteInteg,
  launchLiteInteg,
} from '../e2e_tests/lite/helpers/integ-lifecycle';

/**
 * The KIRO welcome banner is the lite UI's session delimiter: it emits once
 * per lite mount (cold boot + each tui→lite re-entry) and persists at static
 * index 0 — never re-rendered. So N lite mounts => N "· lite" version lines.
 * Regression ref: commit 235c2d6 (dropped the static-banner push, breaking
 * re-emission on remount).
 */

describe('lite welcome banner roundtrip', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it.each([
    { roundtrips: 1, expected: 2 },
    { roundtrips: 2, expected: 3 },
  ])(
    'emits one banner per lite mount across %p roundtrips',
    async ({ roundtrips, expected }) => {
      testCase = await launchLiteInteg(
        `welcome-banner-roundtrip-${roundtrips}`
      );

      const storeBefore = await testCase.getStore();
      expect(storeBefore.uiMode).toBe('lite');
      // First mount: welcome banner is visible and the emit flag is still false.
      const bannerLines = testCase
        .getSnapshot()
        .filter((line) => line.includes('lite'));
      expect(bannerLines.length).toBeGreaterThan(0);
      expect(storeBefore.liteWelcomeEmitted).toBe(false);

      for (let i = 0; i < roundtrips; i++) {
        await switchToTui(testCase);
        await testCase.sleepMs(500);
        const storeTui = await testCase.getStore();
        expect(storeTui.uiMode).toBe('tui');
        // Unmount sets liteWelcomeEmitted=true (gates the live-region banner
        // so the welcome screen doesn't re-flash on the next lite mount).
        expect(storeTui.liteWelcomeEmitted).toBe(true);

        await switchToLite(testCase);
        await testCase.sleepMs(500);
        const storeLite = await testCase.getStore();
        expect(storeLite.uiMode).toBe('lite');
        expect(storeLite.liteWelcomeEmitted).toBe(true);
      }

      // Each mount writes the "· lite" version line exactly once (live-region
      // on the cold welcome screen, static row at index 0 on each remount).
      const versionLineMatches =
        testCase.getOutputCleaned().match(/· lite/g) || [];
      expect(versionLineMatches.length).toBe(expected);

      await exitLiteInteg(testCase);
    },
    30000
  );
});
