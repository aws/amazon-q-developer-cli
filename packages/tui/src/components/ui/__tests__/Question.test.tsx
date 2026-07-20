import { afterEach, describe, expect, test, vi } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import {
  AppStoreContext,
  createAppStore,
  ToolUseStatus,
} from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';
import { Question } from '../Question.js';
import { ToolUseMessage } from '../ToolUseMessage.js';
import type { UserInputOption } from '@kiro/acp-type-covenant';

const DOWN = '\x1b[B';
const ENTER = '\r';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  output = '';
  columns = 80;
  rows = 24;
  kittyProtocolActive = true;
  start(onInput: (data: string) => void): void {
    this.onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
  }
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
  sendInput(data: string): void {
    this.onInput?.(data);
  }
}

let instance: Instance | null = null;

afterEach(() => {
  instance?.unmount();
  instance = null;
});

async function flush(): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

function mountQuestion(options: UserInputOption[]) {
  const terminal = new MockTerminal();
  const onAnswer = vi.fn(() => true);
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  instance = render(
    <AppStoreContext.Provider value={store}>
      <Question
        question="**Which path?**"
        options={options}
        onAnswer={onAnswer}
        onCancel={vi.fn()}
      />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  return { terminal, onAnswer };
}

const OPTIONS = [{ title: 'First' }, { title: 'Second' }, { title: 'Third' }];

describe('Question keyboard behavior', () => {
  test('number shortcuts highlight and Enter confirms', async () => {
    const { terminal, onAnswer } = mountQuestion(OPTIONS);
    await flush();

    terminal.sendInput('3');
    await flush();
    expect(terminal.output).toContain('❯ 3. Third');
    expect(onAnswer).not.toHaveBeenCalled();

    terminal.sendInput(ENTER);
    await flush();
    expect(onAnswer).toHaveBeenCalledWith('Third');
  });

  test('typing text immediately enters and submits a custom response', async () => {
    const { terminal, onAnswer } = mountQuestion(OPTIONS);
    await flush();

    for (const character of 'custom 3 answer') terminal.sendInput(character);
    terminal.sendInput(ENTER);
    await flush();

    expect(onAnswer).toHaveBeenCalledWith('custom 3 answer');
  });

  test('a continued number shortcut is resolved for the agent', async () => {
    const { terminal, onAnswer } = mountQuestion(OPTIONS);
    await flush();

    for (const character of '1 but add context') terminal.sendInput(character);
    terminal.sendInput(ENTER);
    await flush();

    expect(onAnswer).toHaveBeenCalledWith(
      '1 but add context',
      'First but add context'
    );
  });

  test('numbers remain literal after navigating to the custom row', async () => {
    const { terminal, onAnswer } = mountQuestion(OPTIONS);
    await flush();

    terminal.sendInput(DOWN);
    terminal.sendInput(DOWN);
    terminal.sendInput(DOWN);
    for (const character of '1 but literal') terminal.sendInput(character);
    terminal.sendInput(ENTER);
    await flush();

    expect(onAnswer).toHaveBeenCalledWith('1 but literal');
  });
});

test('question transcript renders markdown instead of raw markers', async () => {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  const message = {
    id: 'question-1',
    name: '**Requirement 7.3:** What should happen on retry?',
    content: '{}',
    isQuestion: true,
    isFinished: true,
    isStatic: true,
    status: ToolUseStatus.Rejected,
    result: { status: 'cancelled' as const },
  };
  instance = render(
    <AppStoreContext.Provider value={store}>
      <ToolUseMessage {...message} />
    </AppStoreContext.Provider>,
    { terminal, exitOnCtrlC: false }
  );
  await flush();

  expect(terminal.output).toContain('Requirement 7.3');
  expect(terminal.output).not.toContain('**Requirement 7.3:**');
  expect(terminal.output).toContain('Cancelled');
});
