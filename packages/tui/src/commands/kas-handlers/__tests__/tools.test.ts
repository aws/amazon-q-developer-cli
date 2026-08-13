import { describe, it, expect, mock } from 'bun:test';
import { handleTools } from '../tools';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import { KasCommandName } from '../../../kas-commands';
import type { ToolInfo } from '../../../stores/app-store';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../../../kiro']);

mock.module('../../../kiro', () => ({
  Kiro: mock(() => ({ initialize: mock() })),
}));

const toolsCmd = {
  name: KasCommandName.Tools,
  description: 'List available tools',
  meta: { inputType: 'panel' },
};

describe('/tools KAS handler', () => {
  it('opens the panel with the cached toolsList snapshot', async () => {
    const toolsList: ToolInfo[] = [
      { name: 'read', source: 'builtin', description: 'read tools' },
      { name: '@git/status', source: 'mcp', description: 'git status' },
    ];
    const ctx = createMockCommandContext({ toolsList });
    (ctx as any).agentEngine = 'kas';

    await handleTools(toolsCmd as any, '', ctx);

    expect(ctx._spies.setShowToolsPanel).toHaveBeenCalledTimes(1);
    const [show, passedTools] = ctx._spies.setShowToolsPanel!.mock.calls[0]!;
    expect(show).toBe(true);
    expect(passedTools).toEqual(toolsList);
  });

  it('opens the panel with an empty list when no tools cached', async () => {
    const ctx = createMockCommandContext({ toolsList: [] });
    (ctx as any).agentEngine = 'kas';

    await handleTools(toolsCmd as any, '', ctx);

    const [show, passedTools] = ctx._spies.setShowToolsPanel!.mock.calls[0]!;
    expect(show).toBe(true);
    expect(passedTools).toEqual([]);
  });

  it('passes a copy, not the live snapshot reference', async () => {
    const toolsList: ToolInfo[] = [
      { name: 'write', source: 'builtin', description: 'write tools' },
    ];
    const ctx = createMockCommandContext({ toolsList });
    await handleTools(toolsCmd as any, '', ctx);
    const [, passedTools] = ctx._spies.setShowToolsPanel!.mock.calls[0]!;
    expect(passedTools).not.toBe(toolsList);
    expect(passedTools).toEqual(toolsList);
  });
});
