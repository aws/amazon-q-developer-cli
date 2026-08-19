/**
 * Telemetry identity for KAS agent-side OTel pipeline.
 *
 * KAS owns its OTel pipeline, while the client supplies the identity fields
 * negotiated at initialization.
 */

import { createHash } from 'node:crypto';
import { getCliVersion } from './version';

let telemetryUserId = '';
let telemetryIdentityChangeHandler: () => void = () => {};

const PSEUDONYMOUS_USER_ID = /^v1:[A-Za-z0-9_-]{43}$/;
const USER_ID_DOMAIN = Buffer.from('kiro-tui-telemetry-user-id:v1\0');
const CONTROL_CHARACTER = /\p{Cc}/u;

export function pseudonymousTelemetryUserId(rawUserId: string): string {
  return `v1:${createHash('sha256')
    .update(USER_ID_DOMAIN)
    .update(rawUserId)
    .digest('base64url')}`;
}

export function isPseudonymousTelemetryUserId(userId: string): boolean {
  return PSEUDONYMOUS_USER_ID.test(userId);
}

function validatedRawUserId(userId: string): string | undefined {
  if (
    userId.trim().length === 0 ||
    // Mirror kiro-telemetry `is_valid_raw_user_id` (MAX_RAW_USER_ID_BYTES = 512)
    // so the TUI and the Rust host never disagree about whether a raw id is usable.
    Buffer.byteLength(userId, 'utf8') > 512 ||
    CONTROL_CHARACTER.test(userId)
  ) {
    return undefined;
  }
  return userId;
}

function consumeEnvironmentUserId(): void {
  const userId = process.env['KIRO_USER_ID'];
  if (userId === undefined) return;
  const validated = validatedRawUserId(userId);
  telemetryUserId =
    validated === undefined ? '' : pseudonymousTelemetryUserId(validated);
  delete process.env['KIRO_USER_ID'];
}

consumeEnvironmentUserId();

export function registerTelemetryIdentityChangeHandler(
  handler: () => void
): void {
  telemetryIdentityChangeHandler = handler;
}

export function setTelemetryUserId(userId: string | undefined): void {
  if (userId !== undefined && !isPseudonymousTelemetryUserId(userId)) return;
  const nextUserId = userId ?? '';
  if (telemetryUserId === nextUserId) return;
  telemetryUserId = nextUserId;
  telemetryIdentityChangeHandler();
}

export interface TelemetryIdentity {
  machineId: string;
  userId: string;
  version: string;
  kiroClientVersion: string;
  channel: string;
}

export function getTelemetryIdentity(): TelemetryIdentity {
  consumeEnvironmentUserId();
  const version = getCliVersion();
  return {
    machineId:
      process.env['KIRO_TELEMETRY_CLIENT_ID']?.trim() ||
      'UNDETERMINED_MACHINE_ID',
    userId: telemetryUserId,
    version,
    kiroClientVersion: version,
    channel: version.includes('-nightly')
      ? 'nightly'
      : version.includes('beta')
        ? 'beta'
        : 'stable',
  };
}

export function isTelemetryEnabled(): boolean {
  if (process.env['KIRO_DISABLE_TELEMETRY']) return false;
  return process.env['KIRO_TELEMETRY_ENABLED'] !== 'false';
}
