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
 * Returns the CLI version.
 *
 * Resolution order:
 *   1. KIRO_VERSION_OVERRIDE — test-only escape hatch for version-gated
 *      features before a release.
 *   2. KIRO_VERSION — injected by the Rust host (chat-cli / chat-cli-v2)
 *      when it spawns the bun subprocess. This carries the real
 *      CARGO_PKG_VERSION (set at build time from KIRO_VERSION on stable /
 *      toolbox releases). The bundled package.json below is pinned to
 *      "0.0.0-dev" in-repo and is NOT bumped by the tag-based release flow,
 *      so without this env the footer showed 0.0.0-dev on real releases.
 *   3. DEV_FALLBACK_VERSION — when package.json is still the "0.0.0-dev"
 *      placeholder (pure-source/dev runs like `bun run dev` where no Rust
 *      host set KIRO_VERSION), map to a high dev version so KRS doesn't
 *      disable thinking (gated on >= 2.4.0).
 *   4. package.json.version — final fallback.
 */
export function getCliVersion(): string {
  const override = process.env.KIRO_VERSION_OVERRIDE?.trim();
  if (override) return override;
  const hostVersion = process.env.KIRO_VERSION;
  if (hostVersion) return hostVersion;
  if (packageJson.version === PLACEHOLDER_VERSION) return DEV_FALLBACK_VERSION;
  return packageJson.version;
}
