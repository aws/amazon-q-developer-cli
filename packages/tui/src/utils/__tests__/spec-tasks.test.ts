import { describe, expect, it } from 'bun:test';
import {
  parseSpecTasks,
  taskStatusFromExecution,
  trayTaskId,
} from '../spec-tasks.js';

const required = { includeOptional: false };

describe('parseSpecTasks', () => {
  it('reads tasks numbered without a trailing dot', () => {
    const md = [
      '- [x] 1.1 Handle the update',
      '- [ ] 1.2 Guard the broadcast',
    ].join('\n');
    expect(parseSpecTasks(md, required)).toEqual([
      { id: '1.1', subject: 'Handle the update', status: 'completed' },
      { id: '1.2', subject: 'Guard the broadcast', status: 'pending' },
    ]);
  });

  it('reads tasks numbered with a trailing dot', () => {
    const md = '- [ ] 2. Build the thing';
    expect(parseSpecTasks(md, required)).toEqual([
      { id: '2', subject: 'Build the thing', status: 'pending' },
    ]);
  });

  it('treats a capital X as completed', () => {
    const md = '- [X] 1. Done';
    expect(parseSpecTasks(md, required)[0]?.status).toBe('completed');
  });

  it('maps in-progress and queued checkbox marks', () => {
    const md = ['- [-] 1. Running now', '- [~] 2. Queued up'].join('\n');
    expect(parseSpecTasks(md, required).map((t) => t.status)).toEqual([
      'running',
      'pending',
    ]);
  });

  it('keeps only leaf tasks when numbering implies a hierarchy', () => {
    const md = [
      '- [ ] 1. Parent',
      '  - [ ] 1.1 First child',
      '  - [ ] 1.2 Second child',
      '- [ ] 2. Standalone',
    ].join('\n');
    expect(parseSpecTasks(md, required).map((t) => t.id)).toEqual([
      '1.1',
      '1.2',
      '2',
    ]);
  });

  it('reads hierarchy from numbering even when the file is flat', () => {
    const md = ['- [ ] 1. Parent', '- [ ] 1.1 Child'].join('\n');
    expect(parseSpecTasks(md, required).map((t) => t.id)).toEqual(['1.1']);
  });

  it('falls back to indentation when nothing is numbered', () => {
    const md = ['- [ ] Parent', '  - [ ] Child'].join('\n');
    expect(parseSpecTasks(md, required)).toEqual([
      { id: 'Child', subject: 'Child', status: 'pending' },
    ]);
  });

  it('excludes optional tasks unless the run promotes them', () => {
    const md = ['- [ ] 1. Required', '- [ ]* 2. Optional'].join('\n');
    expect(parseSpecTasks(md, required).map((t) => t.id)).toEqual(['1']);
    expect(
      parseSpecTasks(md, { includeOptional: true }).map((t) => t.id)
    ).toEqual(['1', '2']);
  });

  it('recognises the escaped optional marker', () => {
    const md = '- [ ]\\* 2. Optional';
    expect(parseSpecTasks(md, required)).toEqual([]);
  });

  it('accepts asterisk and plus bullets', () => {
    const md = ['* [ ] 1. Star', '+ [ ] 2. Plus'].join('\n');
    expect(parseSpecTasks(md, required).map((t) => t.id)).toEqual(['1', '2']);
  });

  it('ignores prose and headings', () => {
    const md = [
      '# Implementation Tasks',
      '',
      '**Validates: Requirements 1.1**',
      '- [ ] 1. Real task',
      'some trailing note',
    ].join('\n');
    expect(parseSpecTasks(md, required).map((t) => t.id)).toEqual(['1']);
  });

  it('handles CRLF line endings', () => {
    const md = '- [ ] 1. First\r\n- [ ] 2. Second';
    expect(parseSpecTasks(md, required).map((t) => t.subject)).toEqual([
      'First',
      'Second',
    ]);
  });
});

describe('trayTaskId', () => {
  it('keys a numbered task by its number', () => {
    expect(trayTaskId('1.1 Handle the update')).toBe('1.1');
    expect(trayTaskId('2. Build the thing')).toBe('2');
  });

  it('agrees with the id parseSpecTasks assigned', () => {
    const md = '- [ ] 1.2 Guard the broadcast';
    const [task] = parseSpecTasks(md, required);
    expect(task).toBeDefined();
    expect(trayTaskId('1.2 Guard the broadcast')).toBe(task!.id);
  });

  it('falls back to the text when the task is unnumbered', () => {
    expect(trayTaskId('  Just a task  ')).toBe('Just a task');
  });
});

describe('taskStatusFromExecution', () => {
  it('maps the statuses the agent reports', () => {
    expect(taskStatusFromExecution('running', 'pending')).toBe('running');
    expect(taskStatusFromExecution('succeed', 'running')).toBe('completed');
    expect(taskStatusFromExecution('failed', 'running')).toBe('failed');
  });

  it('returns a queued or aborted task to not-started', () => {
    expect(taskStatusFromExecution('queued', 'pending')).toBe('pending');
    expect(taskStatusFromExecution('aborted', 'running')).toBe('pending');
  });

  it('leaves the status alone when the agent yields', () => {
    expect(taskStatusFromExecution('yielded', 'running')).toBe('running');
  });
});
