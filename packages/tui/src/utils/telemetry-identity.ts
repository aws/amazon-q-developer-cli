/**
 * Telemetry identity for KAS agent-side OTel pipeline.
 *
 * KAS owns its OTel pipeline, while the client supplies the identity fields
 * negotiated at initialization.
 */

import { getCliVersion } from './version';

export interface TelemetryIdentity {
  machineId: string;
  userId: string;
  version: string;
  kiroClientVersion: string;
}

export function getTelemetryIdentity(): TelemetryIdentity {
  const version = getCliVersion();
  return {
    machineId:
      process.env['KIRO_TELEMETRY_CLIENT_ID']?.trim() ||
      'UNDETERMINED_MACHINE_ID',
    userId: process.env['KIRO_USER_ID'] || '',
    version,
    kiroClientVersion: version,
  };
}

export function isTelemetryEnabled(): boolean {
  if (process.env['KIRO_DISABLE_TELEMETRY']) return false;
  return process.env['KIRO_TELEMETRY_ENABLED'] !== 'false';
}
