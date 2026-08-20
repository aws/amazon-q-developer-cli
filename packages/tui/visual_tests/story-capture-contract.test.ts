import { describe, expect, test } from 'bun:test';
import {
  mergeStoryAssertions,
  missingCaptureIds,
  rendererFailureMessages,
  resolveCaptureDefinition,
} from '../src/storybook/story-capture-contract.js';

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
        },
        {
          visible: ['Thinking...'],
          hidden: ['failed'],
          ordered: ['reply'],
          occurrences: { reply: 1 },
        }
      )
    ).toEqual({
      visible: ['WORKFLOW OUTPUT', 'Thinking...'],
      hidden: ['undefined', 'failed'],
      ordered: ['sent', 'reply'],
      occurrences: { sent: 1, reply: 1 },
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
});
