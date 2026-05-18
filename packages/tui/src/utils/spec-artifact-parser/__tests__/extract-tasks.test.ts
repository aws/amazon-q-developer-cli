import { describe, it, expect } from 'bun:test';
import { extractTasks } from '../extract-tasks';

describe('extractTasks', () => {
  it('returns empty for empty input', () => {
    expect(extractTasks('')).toEqual([]);
  });

  it('returns empty when no checkbox lines exist', () => {
    expect(extractTasks('# Tasks\n\nLorem ipsum.')).toEqual([]);
  });

  it('extracts a single high-level task', () => {
    const md = '- [ ] 1. Build the parser';
    const tasks = extractTasks(md);
    expect(tasks).toHaveLength(1);
    expect(tasks[0]!.number).toBe('1');
    expect(tasks[0]!.title).toBe('Build the parser');
    expect(tasks[0]!.checked).toBe(false);
    expect(tasks[0]!.subTasks).toEqual([]);
  });

  it('captures checked state correctly', () => {
    const md = [
      '- [x] 1. Done task',
      '- [X] 2. Capital X also done',
      '- [ ] 3. Pending',
    ].join('\n');
    const tasks = extractTasks(md);
    expect(tasks[0]!.checked).toBe(true);
    expect(tasks[1]!.checked).toBe(true);
    expect(tasks[2]!.checked).toBe(false);
  });

  it('groups sub-tasks under their parent', () => {
    const md = [
      '- [ ] 1. Parent A',
      '  - [ ] 1.1. Child A1',
      '  - [x] 1.2. Child A2 done',
      '- [ ] 2. Parent B',
      '  - [ ] 2.1. Child B1',
    ].join('\n');
    const tasks = extractTasks(md);
    expect(tasks).toHaveLength(2);
    expect(tasks[0]!.subTasks).toHaveLength(2);
    expect(tasks[0]!.subTasks[0]!.title).toBe('1.1. Child A1');
    expect(tasks[0]!.subTasks[1]!.checked).toBe(true);
    expect(tasks[1]!.subTasks).toHaveLength(1);
    expect(tasks[1]!.subTasks[0]!.title).toBe('2.1. Child B1');
  });

  it('handles tabs in indentation (expanded to 4 spaces)', () => {
    const md = ['- [ ] 1. Parent', '\t- [ ] 1.1. Child'].join('\n');
    const tasks = extractTasks(md);
    expect(tasks).toHaveLength(1);
    expect(tasks[0]!.subTasks).toHaveLength(1);
    expect(tasks[0]!.subTasks[0]!.depth).toBe(4); // expanded from \t
  });

  it('uses min-depth as the high-level depth', () => {
    // All checkbox lines at depth 2 — they're all high-level then
    const md = [
      '  - [ ] 1. A',
      '  - [ ] 2. B',
      '    - [ ] 1.1. nested under A',
    ].join('\n');
    const tasks = extractTasks(md);
    expect(tasks).toHaveLength(2);
    expect(tasks[0]!.number).toBe('1');
    expect(tasks[1]!.number).toBe('2');
    expect(tasks[0]!.subTasks).toHaveLength(1);
  });

  it('captures detail body verbatim from task line to next task', () => {
    const md = [
      '- [ ] 1. First',
      '  - [ ] 1.1. sub one',
      '  Some inline note',
      '- [ ] 2. Second',
      '  - [ ] 2.1. sub two',
    ].join('\n');
    const tasks = extractTasks(md);
    expect(tasks[0]!.detailBody).toContain('Some inline note');
    expect(tasks[0]!.detailBody).not.toContain('Second');
    expect(tasks[1]!.detailBody).toContain('sub two');
  });

  it('supports tasks without leading numbering', () => {
    const md = '- [ ] Just a task with no number';
    const tasks = extractTasks(md);
    expect(tasks).toHaveLength(1);
    expect(tasks[0]!.number).toBe('');
    expect(tasks[0]!.title).toBe('Just a task with no number');
  });

  it('captures decimal numbering', () => {
    const md = '- [ ] 1.0. Decimal-numbered top-level task';
    const tasks = extractTasks(md);
    expect(tasks[0]!.number).toBe('1.0');
  });

  it('is deterministic', () => {
    const md = [
      '- [ ] 1. A',
      '  - [ ] 1.1. a-sub',
      '- [x] 2. B',
    ].join('\n');
    expect(JSON.stringify(extractTasks(md))).toBe(
      JSON.stringify(extractTasks(md))
    );
  });

  it('does not throw on weird whitespace and replacement chars', () => {
    const md = '- [ ] 1. \uFFFD weird\n  - [ ] 1.1. \u0000 sub';
    expect(() => extractTasks(md)).not.toThrow();
  });
});
