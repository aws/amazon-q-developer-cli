import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { getTelemetryIdentity } from '../telemetry-identity.js';

let originalMachineId: string | undefined;
let originalUserId: string | undefined;

beforeEach(() => {
  originalMachineId = process.env['KIRO_TELEMETRY_CLIENT_ID'];
  originalUserId = process.env['KIRO_USER_ID'];
});

afterEach(() => {
  if (originalMachineId === undefined)
    delete process.env['KIRO_TELEMETRY_CLIENT_ID'];
  else process.env['KIRO_TELEMETRY_CLIENT_ID'] = originalMachineId;

  if (originalUserId === undefined) delete process.env['KIRO_USER_ID'];
  else process.env['KIRO_USER_ID'] = originalUserId;
});

describe('getTelemetryIdentity', () => {
  it('uses the telemetry client ID supplied to the process', () => {
    process.env['KIRO_TELEMETRY_CLIENT_ID'] =
      'ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e';
    process.env['KIRO_USER_ID'] = 'test-user';

    expect(getTelemetryIdentity()).toMatchObject({
      machineId: 'ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e',
      userId: 'test-user',
    });
  });

  it('ignores surrounding whitespace in the supplied client ID', () => {
    process.env['KIRO_TELEMETRY_CLIENT_ID'] =
      '  ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e  ';

    expect(getTelemetryIdentity().machineId).toBe(
      'ed9aa51f-68ef-4048-b2dd-6c02ca3fdc9e'
    );
  });

  it('uses an explicit fallback when no client ID was supplied', () => {
    delete process.env['KIRO_TELEMETRY_CLIENT_ID'];

    expect(getTelemetryIdentity().machineId).toBe('UNDETERMINED_MACHINE_ID');
  });
});
