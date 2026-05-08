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

function resolveChannel(): string {
  const envChannel = process.env['KIRO_UPDATE_CHANNEL'];
  if (envChannel) return envChannel;

  const version: string = packageJson.version;
  if (version.includes('nightly')) return 'nightly';
  if (version.includes('insider') || version.includes('beta')) return 'insider';
  return 'stable';
}

export interface TelemetryIdentity {
  machineId: string;
  userId: string;
  version: string;
  kiroClientVersion: string;
  channel: string;
}

export function getTelemetryIdentity(): TelemetryIdentity {
  const machineId = getMachineId();
  return {
    machineId,
    userId: process.env['KIRO_USER_ID'] || machineId,
    version: packageJson.version,
    kiroClientVersion: packageJson.version,
    channel: resolveChannel(),
  };
}

export function isTelemetryEnabled(): boolean {
  if (process.env['KIRO_DISABLE_TELEMETRY']) return false;
  return process.env['KIRO_TELEMETRY_ENABLED'] !== 'false';
}
