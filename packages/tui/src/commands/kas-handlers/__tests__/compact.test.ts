import { describe, it, expect, mock } from 'bun:test';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import { handleCompact } from '../compact';
import type { KasCommand } from '../../../kas-commands';

const COMPACT_CMD: KasCommand = {
  name: 'compact' as any,
  description: 'Compact conversation',
};

describe('handleCompact', () => {
  it('does not show a success alert because the compaction event owns progress', async () => {
    const executeCommand = mock(() =>
      Promise.resolve({ success: true, message: 'Compacting conversation...' })
    );
    const ctx = createMockCommandContext({
      kiro: { executeCommand } as any,
    });

    await handleCompact(COMPACT_CMD, '', ctx as any);

    expect(executeCommand).toHaveBeenCalledWith({
      command: 'compact',
      args: {},
    });
    expect(ctx._spies.showAlert).not.toHaveBeenCalled();
  });

  it('still surfaces compact command failures', async () => {
    const executeCommand = mock(() =>
      Promise.resolve({ success: false, message: 'kas is down' })
    );
    const ctx = createMockCommandContext({
      kiro: { executeCommand } as any,
    });

    await handleCompact(COMPACT_CMD, '  now  ', ctx as any);

    expect(executeCommand).toHaveBeenCalledWith({
      command: 'compact',
      args: { value: 'now' },
    });
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'kas is down',
      'error',
      5000
    );
  });
});
