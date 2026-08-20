import React from 'react';
import { describe, expect, test, vi } from 'vitest';
import { renderWithProviders } from '../chat/tools/__tests__/twinki-render.js';
import { MessageInput } from './MessageInput.js';

describe('MessageInput', () => {
  test('renders theme-colored text through the renderer text contract', async () => {
    const output = await renderWithProviders(
      <MessageInput
        targetSessionId="release-session"
        targetSessionName="release-verifier"
        onSend={vi.fn()}
        onCancel={vi.fn()}
      />
    );

    expect(output).toContain('Send message to release-verifier');
    expect(output).toContain('Press Enter to send, Esc to cancel');
  });
});
