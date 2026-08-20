import { describe, expect, test } from 'bun:test';
import {
  mergeStoryAssertions,
  missingCaptureIds,
  rendererFailureMessages,
  resolveCaptureDefinition,
} from '../src/storybook/story-capture-contract.js';
import { storyFrameAssertionFailures } from '../src/storybook/story-frame-assertions.js';
import type { TerminalFrame } from '../src/test-utils/shared/terminal-frame.js';

describe('visual story capture contract', () => {
  test('resolves stable ids, labels, and state assertions from metadata', () => {
    const resolved = resolveCaptureDefinition('thinking', {
      thinking: {
        label: 'workflow thinking after steer',
        assertions: { visible: ['Thinking...'] },
      },
    });

    expect(resolved).toEqual({
      id: 'thinking',
      definition: {
        label: 'workflow thinking after steer',
        assertions: { visible: ['Thinking...'] },
      },
    });
  });

  test('rejects undeclared and invalid journey captures', () => {
    expect(() =>
      resolveCaptureDefinition('reply', {
        thinking: { label: 'thinking' },
      })
    ).toThrow('undeclared state "reply"');
    expect(() =>
      resolveCaptureDefinition('Not Stable', {
        'not-stable': { label: 'not stable' },
      })
    ).toThrow('Invalid visual capture id');
  });

  test('keeps legacy labels compatible before baselines are established', () => {
    expect(
      resolveCaptureDefinition('Reply with preserved history', undefined)
    ).toEqual({
      id: 'reply-with-preserved-history',
      definition: { label: 'Reply with preserved history' },
    });
  });

  test('combines invariants with state checks and detects missed states', () => {
    expect(
      mergeStoryAssertions(
        {
          visible: ['WORKFLOW OUTPUT'],
          hidden: ['undefined'],
          ordered: ['sent'],
          occurrences: { sent: 1 },
          styled: [{ text: 'sent', foreground: '#00ff00' }],
        },
        {
          visible: ['Thinking...'],
          hidden: ['failed'],
          ordered: ['reply'],
          occurrences: { reply: 1 },
          styled: [{ text: 'reply', bold: true }],
        }
      )
    ).toEqual({
      visible: ['WORKFLOW OUTPUT', 'Thinking...'],
      hidden: ['undefined', 'failed'],
      ordered: ['sent', 'reply'],
      occurrences: { sent: 1, reply: 1 },
      styled: [
        { text: 'sent', foreground: '#00ff00' },
        { text: 'reply', bold: true },
      ],
    });
    expect(
      missingCaptureIds(
        {
          sent: { label: 'sent' },
          reply: { label: 'reply' },
        },
        new Set(['sent'])
      )
    ).toEqual(['reply']);
  });

  test('rejects renderer crash banners without rejecting tool errors', () => {
    expect(
      rendererFailureMessages([
        'An error occurred in the <BrokenStory> component.',
        'Consider adding an error boundary.',
      ])
    ).toEqual(['renderer failure marker "An error occurred in the <"']);
    expect(
      rendererFailureMessages([
        'The above error occurred in the <BrokenStory> component:',
      ])
    ).toEqual(['renderer failure marker "The above error occurred in the <"']);
    expect(rendererFailureMessages(['Error: permission denied'])).toEqual([]);
  });

  test('asserts targeted terminal styles without snapshotting the frame', () => {
    const frame: TerminalFrame = {
      schemaVersion: 1,
      viewport: { columns: 2, rows: 1 },
      buffer: 'normal',
      cursor: { column: 0, row: 0 },
      styles: [
        {
          foreground: { mode: 'rgb', value: 0x80ffb5 },
          background: { mode: 'rgb', value: 0x2d3a30 },
          bold: true,
          dim: false,
          italic: false,
          underline: false,
          blink: false,
          inverse: false,
          invisible: false,
          strikethrough: false,
          overline: false,
        },
      ],
      rows: [
        [
          { text: '+', width: 1, styleIndex: 0 },
          { text: 'x', width: 1, styleIndex: 0 },
        ],
      ],
    };

    expect(
      storyFrameAssertionFailures(frame, {
        styled: [
          {
            text: '+x',
            foreground: '#80ffb5',
            background: '#2d3a30',
            bold: true,
          },
        ],
      })
    ).toEqual([]);
    expect(
      storyFrameAssertionFailures(frame, {
        styled: [{ text: '+x', foreground: '#ff0000' }],
      })
    ).toEqual(['styled "+x" expected foreground #ff0000, found #80ffb5']);
  });
});
