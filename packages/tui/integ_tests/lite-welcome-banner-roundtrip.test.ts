import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { switchToLite, switchToTui } from '../e2e_tests/lite/helpers/mode-swap';

/**
 * Lite welcome banner persistence across mode swaps.
 *
 * The KIRO welcome banner (ASCII art + version + tip) is the lite UI's
 * session delimiter. It is supposed to:
 *   1. Show on first mount (cold boot welcome screen).
 *   2. Re-appear at the top of scrollback every time the user (re)enters
 *      lite mode — every tui→lite swap is a new "session" the user wants
 *      visually marked, so the banner anchors the start of that scrollback
 *      block.
 *   3. Persist forever once it lands in <Static> at index 0 — chat content
 *      scrolls past it into terminal saved-lines, but the banner row itself
 *      is never re-rendered, never updated, never removed.
 *
 * Mechanism: the live-region banner shows while showWelcomeBanner is true
 * (welcome screen). The moment chat content lands in static (first User
 * message in a fresh session, or any eligible row after a tui→lite clear-
 * token bump), the banner is pushed at index 0 of staticItemsRef. The
 * live-region banner unmounts on the same render via its own gate so the
 * transition is seamless.
 *
 * (Pre-235c2d6: an earlier commit dropped the static-banner push and made
 * the banner live-region only. That regressed the persistence story —
 * banner unmounted the moment the user typed and never re-appeared. This
 * test was originally written against that intermediate behavior; it now
 * asserts the released-toolbox behavior the user has always wanted.)
 */

describe('lite welcome banner roundtrip', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('welcome banner re-appears on remount as a session delimiter (lite→tui→lite)', async () => {
    testCase = await TestCase.builder()
      .withTestName('welcome-banner-roundtrip-redelimits')
      .withLite()
      .withTimeout(15000)
      .launch();

    // Wait for lite mode to render the welcome banner
    await testCase.waitForVisibleText('ask a question', 10000);

    // Confirm we're in lite mode and the welcome banner is visible
    const storeBefore = await testCase.getStore();
    expect(storeBefore.uiMode).toBe('lite');

    // The banner contains "· lite" in the version line — verify it rendered
    const snapshotBefore = testCase.getSnapshot();
    const bannerLinesBefore = snapshotBefore.filter((line) =>
      line.includes('lite')
    );
    expect(bannerLinesBefore.length).toBeGreaterThan(0);

    // liteWelcomeEmitted starts as false (first mount hasn't unmounted yet)
    expect(storeBefore.liteWelcomeEmitted).toBe(false);

    // Switch to TUI (this unmounts LiteLayout, setting liteWelcomeEmitted=true)
    await switchToTui(testCase);
    await testCase.sleepMs(500);

    const storeTui = await testCase.getStore();
    expect(storeTui.uiMode).toBe('tui');
    // After unmount, liteWelcomeEmitted should be true
    expect(storeTui.liteWelcomeEmitted).toBe(true);

    // Switch back to lite
    await switchToLite(testCase);
    await testCase.sleepMs(500);

    const storeAfter = await testCase.getStore();
    expect(storeAfter.uiMode).toBe('lite');
    // The flag remains true — gates the live-region banner so the welcome
    // screen doesn't re-flash. The static-banner push is a separate path
    // (clear-token swap-with-content OR first-content-in-fresh-mount) that
    // re-emits the banner as a session delimiter at the top of the new
    // lite scrollback.
    expect(storeAfter.liteWelcomeEmitted).toBe(true);

    // Count occurrences of the version line in the raw output.
    // Each lite mount writes a banner row exactly once (live-region on the
    // welcome screen of the first mount, static row at the top of the
    // tui→lite remount). Two mounts → two emissions.
    const outputCleaned = testCase.getOutputCleaned();
    const versionLineMatches = outputCleaned.match(/· lite/g) || [];
    expect(versionLineMatches.length).toBe(2);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);

  it('banner emits once per lite mount across multiple roundtrips', async () => {
    testCase = await TestCase.builder()
      .withTestName('welcome-banner-multi-roundtrip')
      .withLite()
      .withTimeout(15000)
      .launch();

    // Wait for first banner appearance
    await testCase.waitForVisibleText('ask a question', 10000);

    const storeInit = await testCase.getStore();
    expect(storeInit.uiMode).toBe('lite');
    expect(storeInit.liteWelcomeEmitted).toBe(false);

    // First round-trip: lite→tui→lite
    await switchToTui(testCase);
    await testCase.sleepMs(500);

    const storeTui1 = await testCase.getStore();
    expect(storeTui1.uiMode).toBe('tui');
    expect(storeTui1.liteWelcomeEmitted).toBe(true);

    await switchToLite(testCase);
    await testCase.sleepMs(500);

    const storeLite2 = await testCase.getStore();
    expect(storeLite2.uiMode).toBe('lite');
    expect(storeLite2.liteWelcomeEmitted).toBe(true);

    // Second round-trip: lite→tui→lite
    await switchToTui(testCase);
    await testCase.sleepMs(500);

    const storeTui2 = await testCase.getStore();
    expect(storeTui2.uiMode).toBe('tui');
    expect(storeTui2.liteWelcomeEmitted).toBe(true);

    await switchToLite(testCase);
    await testCase.sleepMs(500);

    const storeLite3 = await testCase.getStore();
    expect(storeLite3.uiMode).toBe('lite');
    expect(storeLite3.liteWelcomeEmitted).toBe(true);

    // Three lite mounts (cold + two re-entries) → three banner emissions.
    // Each emission anchors the top of that mount's scrollback as a
    // session delimiter; they never re-render once committed, so the
    // count grows exactly with the number of times the user crossed
    // back into lite.
    const outputCleaned = testCase.getOutputCleaned();
    const versionLineMatches = outputCleaned.match(/· lite/g) || [];
    expect(versionLineMatches.length).toBe(3);

    await testCase.sendKeys([0x03, 0x03, 0x03]);
    await testCase.expectExit();
  }, 30000);
});
