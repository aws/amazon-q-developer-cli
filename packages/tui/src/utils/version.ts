import packageJson from '../../package.json';

/**
 * Returns the CLI version. Supports KIRO_VERSION_OVERRIDE env var
 * for testing version-gated features before a release.
 */
export function getCliVersion(): string {
  return process.env.KIRO_VERSION_OVERRIDE ?? packageJson.version;
}
