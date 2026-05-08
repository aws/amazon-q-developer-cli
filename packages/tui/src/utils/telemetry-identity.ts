/**
 * Telemetry identity for KAS agent-side OTel pipeline.
 *
 * The agent (KAS) owns the full OTel pipeline — the client just passes
 * identity fields at initialize time via `clientMeta.telemetry`.
 */

import { machineIdSync } from 'node-machine-id';
import packageJson from '../../package.json';

function getMachineId(): string {
  try {
    return machineIdSync();
  } catch {
    return 'UNDETERMINED_MACHINE_ID';
  }
}

/** Strict SemVer: X.Y.Z (no prerelease suffix) */
const SEMVER_STABLE = /^\d+\.\d+\.\d+$/;

function resolveChannel(): string {
  const envChannel = process.env['KIRO_UPDATE_CHANNEL'];
  if (envChannel) return envChannel;

  const version: string = packageJson.version;
  if (version.includes('nightly')) return 'nightly';
  if (version.includes('beta') || version.includes('insider')) return 'beta';
  // Stable = public installations and toolbox (strict SemVer X.Y.Z)
  if (SEMVER_STABLE.test(version)) return 'stable';
  return 'unknown';
}

export interface TelemetryIdentity {
  machineId: string;
  userId: string;
  version: string;
  kiroClientVersion: string;
  channel: string;
}

/**
 * KIRO_USER_ID is set by the Rust launcher in embedded_tui.rs from
 * get_usage_limits().user_info().user_id() — only available when authenticated.
 */
export function getTelemetryIdentity(): TelemetryIdentity {
  const machineId = getMachineId();
  return {
    machineId,
    userId: process.env['KIRO_USER_ID'] || '',
    version: packageJson.version,
    kiroClientVersion: packageJson.version,
    channel: resolveChannel(),
  };
}

export function isTelemetryEnabled(): boolean {
  if (process.env['KIRO_DISABLE_TELEMETRY']) return false;
  return process.env['KIRO_TELEMETRY_ENABLED'] !== 'false';
}
