import { describe, it, expect } from 'bun:test';
import {
  renderSubagentFinalBlock,
  formatSubagentApprovalLines,
} from '../render.js';
import { DEFAULT_DISPLAY, DENSITY_DISPLAY } from '../verbose.js';

// eslint-disable-next-line no-control-regex
const stripAnsi = (s: string) => s.replace(/\u001b\[[0-9;]*m/g, '');

// A pipeline where the single stage's prompt embeds {task} (the normal
// pattern). The substituted task text is rendered inside the pipeline, so the
// standalone `task:` line above it would be a duplicate.
const contentWithTaskPlaceholder = JSON.stringify({
  task: 'Investigate the flush bug',
  stages: [{ name: 'scan', prompt_template: 'Do this: {task}' }],
});

// A pipeline whose prompt does NOT reference {task}. The task: line is still
// dropped (the task duplicates the user's input prompt); the stage prompt is
// shown verbatim in the pipeline.
const contentNoPlaceholder = JSON.stringify({
  task: 'Investigate the flush bug',
  stages: [{ name: 'scan', prompt_template: 'Read the static-flush module' }],
});

describe('renderSubagentFinalBlock — Bug 3 cancelled tail', () => {
  it('renders a terminal "✗ cancelled" suffix, not the running "..."', () => {
    const out = stripAnsi(
      renderSubagentFinalBlock(
        contentWithTaskPlaceholder,
        { status: 'cancelled' },
        'cancelled',
        undefined,
        undefined,
        { display: DEFAULT_DISPLAY }
      )
    );
    const header = out.split('\n')[0]!;
    expect(header).toContain('✗ cancelled');
    expect(header).not.toMatch(/subagent\s*\.\.\.$/);
  });
});

describe('renderSubagentFinalBlock — Bug 2 task line dropped', () => {
  it('omits the standalone "task:" line (prompts on); task still shown in pipeline', () => {
    const out = stripAnsi(
      renderSubagentFinalBlock(
        contentWithTaskPlaceholder,
        undefined,
        'running',
        undefined,
        undefined,
        { display: DEFAULT_DISPLAY }
      )
    );
    // No standalone "task:" key line...
    expect(out).not.toMatch(/^\s*task:/m);
    // ...but the task text still appears (rendered inside the pipeline prompt).
    expect(out).toContain('Investigate the flush bug');
  });

  it('omits the standalone "task:" line even when no stage embeds {task}', () => {
    const out = stripAnsi(
      renderSubagentFinalBlock(
        contentNoPlaceholder,
        undefined,
        'running',
        undefined,
        undefined,
        { display: DEFAULT_DISPLAY }
      )
    );
    expect(out).not.toMatch(/^\s*task:/m);
  });

  it('omits the standalone "task:" line in minimal preset (prompts hidden)', () => {
    const out = stripAnsi(
      renderSubagentFinalBlock(
        contentWithTaskPlaceholder,
        undefined,
        'running',
        undefined,
        undefined,
        { display: DENSITY_DISPLAY.minimal }
      )
    );
    expect(out).not.toMatch(/^\s*task:/m);
  });
});

describe('formatSubagentApprovalLines — Bug 2 task line dropped', () => {
  it('omits the standalone "task:" line; task still shown in pipeline', () => {
    const lines = formatSubagentApprovalLines(contentWithTaskPlaceholder, 80)!;
    const out = stripAnsi(lines.join('\n'));
    expect(out).not.toMatch(/^\s*task:/m);
    expect(out).toContain('Investigate the flush bug');
  });

  it('omits the standalone "task:" line even when no stage embeds {task}', () => {
    const lines = formatSubagentApprovalLines(contentNoPlaceholder, 80)!;
    const out = stripAnsi(lines.join('\n'));
    expect(out).not.toMatch(/^\s*task:/m);
  });
});
