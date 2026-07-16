import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';
import { handleDisconnect } from '../disconnect';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import { resetCloudDetachNoticeForTest } from '../../../utils/cloud-detach-notice';
import { KasCommandName } from '../../../kas-commands';
import type { KasCommand } from '../../../kas-commands';

const CMD: KasCommand = {
  name: KasCommandName.Disconnect,
  description: 'Detach from the cloud session',
  meta: { cloudOnly: true },
};

describe('handleDisconnect', () => {
  let written: string[];
  let origWrite: typeof process.stderr.write;

  beforeEach(() => {
    resetCloudDetachNoticeForTest();
    written = [];
    origWrite = process.stderr.write.bind(process.stderr);
    process.stderr.write = mock((chunk: string | Uint8Array) => {
      written.push(String(chunk));
      return true;
    }) as typeof process.stderr.write;
  });

  afterEach(() => {
    process.stderr.write = origWrite;
    resetCloudDetachNoticeForTest();
  });

  it('cloud-active: prints the reattach notice, closes, then exits with 0', async () => {
    const order: string[] = [];
    const close = mock(() => order.push('close'));
    const exit = mock((code: number) => order.push(`exit:${code}`));
    const ctx = createMockCommandContext({
      kiro: { sessionId: 'sess-abc', close } as any,
    });
    ctx.cloudSessionActive = true;

    await handleDisconnect(CMD, '', ctx, undefined, exit);

    expect(written.some((w) => w.includes('Quit session sess-abc'))).toBe(true);
    expect(order).toEqual(['close', 'exit:0']);
  });

  it('not cloud-active: no detach, no close, no exit', async () => {
    const close = mock(() => {});
    const exit = mock(() => {});
    const ctx = createMockCommandContext({
      kiro: { sessionId: 'sess-abc', close } as any,
    });
    ctx.cloudSessionActive = false;

    await handleDisconnect(CMD, '', ctx, undefined, exit);

    expect(exit).not.toHaveBeenCalled();
    expect(close).not.toHaveBeenCalled();
    expect(written).toHaveLength(0);
  });
});
