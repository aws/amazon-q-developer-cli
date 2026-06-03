import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { getCliVersion } from '../version';

/**
 * `getCliVersion()` flows outbound to KAS (ACP `clientInfo.version`),
 * KAS OTel telemetry (`kiroClientVersion`), the Aperture User-Agent, and
 * other backends. KRS gates "thinking" on `appVersion >= 2.4.0`.
 *
 * The TUI ships `0.0.0-dev` in `package.json` as a placeholder for the
 * tag-driven release model — CI rewrites it at build time. Local dev
 * (`bun run dev`) inherits the placeholder, which would silently disable
 * thinking on every dev session. `getCliVersion()` maps it to a high
 * dev version. Don't remove these tests without removing that mapping.
 */
describe('getCliVersion', () => {
  let originalOverride: string | undefined;

  beforeEach(() => {
    originalOverride = process.env.KIRO_VERSION_OVERRIDE;
    delete process.env.KIRO_VERSION_OVERRIDE;
  });

  afterEach(() => {
    if (originalOverride !== undefined) {
      process.env.KIRO_VERSION_OVERRIDE = originalOverride;
    } else {
      delete process.env.KIRO_VERSION_OVERRIDE;
    }
  });

  it('never returns the 0.0.0-dev placeholder', () => {
    expect(getCliVersion()).not.toBe('0.0.0-dev');
  });

  it('respects KIRO_VERSION_OVERRIDE', () => {
    process.env.KIRO_VERSION_OVERRIDE = '2.4.0';
    expect(getCliVersion()).toBe('2.4.0');
  });
});
