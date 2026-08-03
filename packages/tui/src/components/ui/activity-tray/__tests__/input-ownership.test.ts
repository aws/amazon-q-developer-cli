import { describe, expect, it } from 'bun:test';
import type { Key } from '../../../../hooks/useKeypress.js';
import {
  activityTrayOwnsInput,
  resolveActivityTrayInputAction,
  resolveActivityTrayQueueInputAction,
  type ActivityTrayInputAction,
  type ActivityTrayInputOwnership,
} from '../input-ownership.js';

const inactive: ActivityTrayInputOwnership = {
  rowNavigationActive: false,
  tabNavigationActive: false,
  workflowNavigationActive: false,
};

function key(overrides: Partial<Key>): Key {
  return {
    upArrow: false,
    downArrow: false,
    leftArrow: false,
    rightArrow: false,
    pageUp: false,
    pageDown: false,
    home: false,
    end: false,
    return: false,
    escape: false,
    ctrl: false,
    shift: false,
    meta: false,
    tab: false,
    backspace: false,
    delete: false,
    ...overrides,
  };
}

function expectAction(
  input: string,
  pressedKey: Key,
  ownership: ActivityTrayInputOwnership,
  expected: ActivityTrayInputAction | null
): void {
  expect(resolveActivityTrayInputAction(input, pressedKey, ownership)).toBe(
    expected
  );
  expect(activityTrayOwnsInput(input, pressedKey, ownership)).toBe(
    expected !== null
  );
}

describe('activity tray input ownership', () => {
  it('claims only shifted arrows for workflow navigation', () => {
    const ownership = { ...inactive, workflowNavigationActive: true };

    expectAction(
      '',
      key({ shift: true, leftArrow: true }),
      ownership,
      'previous-workflow'
    );
    expectAction(
      '',
      key({ shift: true, rightArrow: true }),
      ownership,
      'next-workflow'
    );
    expectAction(
      '',
      key({ shift: true, upArrow: true }),
      ownership,
      'previous-workflow-node'
    );
    expectAction(
      '',
      key({ shift: true, downArrow: true }),
      ownership,
      'next-workflow-node'
    );
    expectAction('', key({ leftArrow: true }), ownership, null);
    expectAction('n', key({ ctrl: true }), ownership, null);
    expectAction('2', key({}), ownership, null);
  });

  it('claims row-navigation chords without claiming ordinary arrows', () => {
    const ownership = { ...inactive, rowNavigationActive: true };

    expectAction(
      '',
      key({ shift: true, downArrow: true }),
      ownership,
      'next-row'
    );
    expectAction(
      '',
      key({ meta: true, upArrow: true }),
      ownership,
      'previous-row'
    );
    expectAction('p', key({ ctrl: true }), ownership, 'previous-row');
    expectAction('n', key({ ctrl: true }), ownership, 'next-row');
    expectAction('', key({ upArrow: true }), ownership, null);
  });

  it('claims only unmodified Tab for tab navigation', () => {
    const ownership = { ...inactive, tabNavigationActive: true };

    expectAction('\t', key({ tab: true }), ownership, 'next-tab');
    expectAction('\t', key({ tab: true, shift: true }), ownership, null);
    expectAction('\t', key({ tab: true, ctrl: true }), ownership, null);
  });

  it('claims nothing while tray navigation is inactive', () => {
    expectAction(
      'n',
      key({ ctrl: true, shift: true, downArrow: true, tab: true }),
      inactive,
      null
    );
  });

  it('claims queue mutations only from a stable empty prompt', () => {
    expect(
      resolveActivityTrayQueueInputAction(key({ backspace: true }), true, true)
    ).toBe('remove-queue-entry');
    expect(
      resolveActivityTrayQueueInputAction(key({ return: true }), true, true)
    ).toBe('edit-queue-entry');
    expect(
      resolveActivityTrayQueueInputAction(key({ backspace: true }), true, false)
    ).toBeNull();
    expect(
      resolveActivityTrayQueueInputAction(key({ backspace: true }), false, true)
    ).toBeNull();
  });
});
