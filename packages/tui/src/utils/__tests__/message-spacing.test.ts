import { describe, expect, it } from 'bun:test';
import { MessageRole, type MessageType } from '../../stores/app-store';
import { leadingGap } from '../message-spacing';

const user = (steered = false): MessageType => ({
  id: 'u',
  role: MessageRole.User,
  content: 'hi',
  ...(steered ? { steered: true } : {}),
});

const model = (): MessageType => ({
  id: 'm',
  role: MessageRole.Model,
  content: 'out',
});

const tool = (): MessageType => ({
  id: 't',
  role: MessageRole.ToolUse,
  name: 'fs_read',
  content: '{}',
});

describe('leadingGap', () => {
  it('never adds a gap for the first message in a list', () => {
    expect(leadingGap(model(), undefined)).toBe(0);
    expect(leadingGap(user(true), undefined)).toBe(0);
    expect(leadingGap(tool(), undefined)).toBe(0);
  });

  it('sets off a steered user message from the output it interrupts', () => {
    expect(leadingGap(user(true), MessageRole.Model)).toBe(1);
    expect(leadingGap(user(true), MessageRole.ToolUse)).toBe(1);
  });

  it('does not gap a non-steered user message', () => {
    // Regular prompts render via <Message> directly, but guard the policy.
    expect(leadingGap(user(false), MessageRole.Model)).toBe(0);
  });

  it('keeps assistant output flush under the prompt that triggered it', () => {
    expect(leadingGap(model(), MessageRole.User)).toBe(0);
  });

  it('separates assistant output from tool output or a prior assistant chunk', () => {
    expect(leadingGap(model(), MessageRole.ToolUse)).toBe(1);
    expect(leadingGap(model(), MessageRole.Model)).toBe(1);
  });

  it('adds no gap before tool-use messages', () => {
    expect(leadingGap(tool(), MessageRole.Model)).toBe(0);
    expect(leadingGap(tool(), MessageRole.User)).toBe(0);
  });
});
