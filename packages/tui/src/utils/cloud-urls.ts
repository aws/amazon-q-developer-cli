/** Kiro web-portal URLs the cloud-sandbox surfaces link to. */

/** Source-provider settings page — fallback when KAS supplies no setup URL. */
export const SOURCE_PROVIDER_SETUP_URL = 'https://app.kiro.dev/settings/agent';

/** All-sessions view linked from the /sessions picker. */
export const CLOUD_SESSIONS_URL = 'https://app.kiro.dev/session';

/** Local-config upload flow shown in the startup checklist guidance. */
export const CONFIG_UPLOAD_URL = 'https://kiro.dev/config/upload';

/**
 * Guidance printed when a cloud session is launched non-interactively but no
 * source provider is connected. Non-interactive mode has no TUI to host the
 * connect gate and no one to retry it, so the caller prints this and exits
 * non-zero rather than awaiting a gate that can never dismiss. Uses the setup
 * URL KAS supplied, falling back to the settings page when it offered none.
 */
export function formatMissingSourceProviderGuidance(
  setupUrl?: string | null
): string {
  return `Cloud sessions require a connected source provider. Connect one at ${setupUrl || SOURCE_PROVIDER_SETUP_URL}, then re-run.`;
}
