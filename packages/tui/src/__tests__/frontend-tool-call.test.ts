import { describe, expect, it } from 'bun:test';
import {
  createFrontendToolCallCapability,
  FRONTEND_TOOL_CALL_METHOD,
  type FrontendToolCallResponse,
} from '../capabilities/frontend-tool-call';

describe('frontend-tool-call capability', () => {
  it('registers under the _kiro/frontendToolCall wire method', () => {
    const cap = createFrontendToolCallCapability();
    expect(cap.method).toBe(FRONTEND_TOOL_CALL_METHOD);
    expect(cap.method).toBe('_kiro/frontendToolCall');
    expect(cap.key).toBe('frontendToolCall');
    expect(cap.value).toBe(true);
  });

  it('declines an unhosted client tool with { outcome: "cancelled" } (never hangs)', async () => {
    const cap = createFrontendToolCallCapability();

    const res = (await cap.handler({
      sessionId: 'spc-9f2',
      toolCallId: 'tc-1',
      title: 'some client tool',
      rawInput: { foo: 'bar' },
    })) as FrontendToolCallResponse;

    expect(res).toEqual({ outcome: 'cancelled' });
  });

  it('declines even with minimal params (no title/rawInput)', async () => {
    const cap = createFrontendToolCallCapability();

    const res = (await cap.handler({
      sessionId: 'spc-9f2',
      toolCallId: 'tc-2',
    })) as FrontendToolCallResponse;

    expect(res.outcome).toBe('cancelled');
  });
});
