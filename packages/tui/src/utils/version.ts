import packageJson from '../../package.json';

const PLACEHOLDER_VERSION = '0.0.0-dev';

/**
 * Hardcoded fallback used only in local dev (placeholder package.json,
 * no KIRO_VERSION_OVERRIDE). Stays above any plausible backend gate so
 * KRS does not silently disable thinking on dev sessions.
 *
 * If you need a more realistic dev version, set KIRO_VERSION_OVERRIDE.
 */
const DEV_FALLBACK_VERSION = '99.99.99-dev';

/**
 * Returns the CLI version. Supports KIRO_VERSION_OVERRIDE env var
 * for testing version-gated features before a release.
 *
 * The package.json ships `0.0.0-dev` as a placeholder for the
 * tag-driven release model — CI rewrites it at build time. Local
 * dev runs (`bun run dev`) inherit the placeholder, which would
 * cause KRS to disable thinking (gated on >= 2.4.0). Map it to a
 * high dev version so outbound traffic always reports a usable value.
 */
export function getCliVersion(): string {
  const override = process.env.KIRO_VERSION_OVERRIDE;
  if (override) return override;
  if (packageJson.version === PLACEHOLDER_VERSION) return DEV_FALLBACK_VERSION;
  return packageJson.version;
}
