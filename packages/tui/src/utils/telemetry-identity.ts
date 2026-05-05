/**
 * Telemetry identity for KAS agent-side OTel pipeline.
 *
 * The agent (KAS) owns the full OTel pipeline — the client just passes
 * identity fields at initialize time via `clientMeta.telemetry`.
 *
 * Fields:
 * - machineId: persistent UUID identifying this device
 * - userId: same as machineId (no user-level auth distinction in CLI)
 * - version: agent package version
 * - kiroClientVersion: TUI package version
 * - channel: update channel (stable | insider | nightly)
 */

import { randomUUID } from 'node:crypto';
import { readCliSettings, writeCliSettings } from './cli-settings.js';
import packageJson from '../../package.json';

const MACHINE_ID_KEY = 'telemetry.machineId';

function getOrCreateMachineId(): string {
  const settings = readCliSettings();
  const existing = settings[MACHINE_ID_KEY];
  if (typeof existing === 'string' && existing.length > 0) {
    return existing;
  }
  const id = randomUUID();
  writeCliSettings({ ...settings, [MACHINE_ID_KEY]: id });
  return id;
}

function resolveChannel(): string {
  // Env var override for CI/testing
  const envChannel = process.env['KIRO_UPDATE_CHANNEL'];
  if (envChannel) return envChannel;

  // Nightly builds have pre-release suffix (e.g. 0.14.0-nightly.1)
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
  const machineId = getOrCreateMachineId();
  return {
    machineId,
    userId: machineId,
    version: packageJson.version,
    kiroClientVersion: packageJson.version,
    channel: resolveChannel(),
  };
}
