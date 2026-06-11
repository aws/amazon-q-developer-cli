import { describe, it, expect, afterEach } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';

/**
 * Regression test for KAS tool rendering in the TUI.
 *
 * KAS sends tool TITLES ('Read File', 'Write File', 'Replace in File') as the
 * tool name, and uses arg shapes that differ from the legacy fs_read/fs_write
 * tools (read_file: flat {path,offset,limit}; fs_write/fs_append: {path,text};
 * str_replace: {path,oldStr,newStr}). These must route to the Read/Write
 * components and render as clean headers/diffs — NOT fall through to the
 * generic Tool renderer that dumps the raw JSON result.
 */
describe('KAS tool rendering', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('renders KAS read/str_replace/fs_write as clean headers and diffs', async () => {
    testCase = await TestCase.builder()
      .withTestName('kas-tool-rendering')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    // KAS read_file — title "Read File", flat { path, offset, limit }
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tc-read',
      name: 'Read File',
      kind: 'read',
      args: { path: 'src/index.ts', offset: 9, limit: 20 },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tc-read',
      // The raw result must NOT leak into the UI.
      result: { status: 'success', output: { message: '<file>secret body</file>' } },
    });

    // KAS str_replace — title "Replace in File", { path, oldStr, newStr }
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tc-replace',
      name: 'Replace in File',
      kind: 'edit',
      args: {
        path: 'src/app.ts',
        oldStr: 'const x = 1;\nconst y = 2;\n',
        newStr: 'const x = 10;\nconst y = 2;\nconst z = 3;\n',
      },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tc-replace',
      result: { status: 'success', output: { ok: true } },
    });

    // KAS fs_write — title "Write File", { path, text } (content in `text`)
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tc-write',
      name: 'Write File',
      kind: 'edit',
      args: {
        path: 'src/new-file.ts',
        text: 'export const hello = () => {\n  return "world";\n};\n',
      },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tc-write',
      result: { status: 'success', output: { ok: true } },
    });

    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-final',
      content: { type: ContentType.Text, text: 'Done.' },
    });

    await testCase.sendKeys('go\r');
    await testCase.completeTurn();
    await testCase.waitForVisibleText('Done.');

    const flat = testCase.getSnapshot().join('\n');

    // Read: clean header with derived line range, no redundant offset/limit meta.
    expect(flat).toContain('Read src/index.ts (L10-29)');
    expect(flat).not.toContain('offset=9');
    expect(flat).not.toContain('limit=20');

    // The raw tool result must never be dumped to the UI.
    expect(flat).not.toContain('secret body');
    expect(flat).not.toContain('"message"');

    // str_replace renders a real diff.
    expect(flat).toContain('Write src/app.ts');
    expect(flat).toContain('const x = 10;');
    expect(flat).toContain('removed 2 lines');

    // fs_write content (in `text`) renders as an added diff.
    expect(flat).toContain('Write src/new-file.ts');
    expect(flat).toContain('export const hello');

  }, 40000);

  it('renders a failed KAS read with a friendly "Read" label, not the raw id', async () => {
    testCase = await TestCase.builder()
      .withTestName('kas-tool-rendering-failed-read')
      .withTimeout(15000)
      .launch();

    await testCase.waitForVisibleText('ask a question');

    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCall,
      id: 'tc-read-err',
      name: 'read_files',
      kind: 'read',
      args: { paths: ['packages/tui/TESTING.md'] },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.ToolCallFinished,
      id: 'tc-read-err',
      result: { status: 'error', error: 'ENOENT: no such file or directory' },
    });
    await testCase.mockSessionUpdate({
      type: AgentEventType.Content,
      id: 'content-retry',
      content: { type: ContentType.Text, text: 'Retrying.' },
    });

    await testCase.sendKeys('go\r');
    await testCase.completeTurn();
    await testCase.waitForVisibleText('Retrying.');

    const flat = testCase.getSnapshot().join('\n');

    // Friendly label, not the raw tool id.
    expect(flat).toContain('Read');
    expect(flat).not.toContain('read_files');
    // Error message surfaces.
    expect(flat).toContain('ENOENT');

  }, 40000);
});
