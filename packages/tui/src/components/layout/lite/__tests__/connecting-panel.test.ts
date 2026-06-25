import { describe, expect, test } from 'bun:test';
import stripAnsi from 'strip-ansi';
import { renderPendingAgent } from '../ConnectingPanel.js';
import {
  KAS_DEFAULT_AGENT_ID,
  KAS_DEFAULT_AGENT_NAME,
} from '../../../../constants/agents.js';

const passthrough = (s: string) => s;

describe('renderPendingAgent', () => {
  test('uses the canonical KAS default agent display label', () => {
    const out = stripAnsi(
      renderPendingAgent(KAS_DEFAULT_AGENT_ID, 0, () => passthrough, ['*'])
    );

    expect(out).toBe(`* ${KAS_DEFAULT_AGENT_NAME}`);
  });
});
