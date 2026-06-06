/**
 * E2E test: ValidationException errors (like IMAGE_SIZE_EXCEEDED) must surface
 * the service's user-friendly message to the TUI, not just the raw reason code.
 *
 * When the API returns a ValidationException with a message like
 * "Image exceeds maximum allowed size of 3.75MB", the user should see that
 * message in the transient alert — not "IMAGE_SIZE_EXCEEDED".
 *
 * Parameterized to run in both TUI and Lite modes via describe.each.
 */

import { afterEach, describe, expect, it } from 'bun:test';
import { E2ETestCase } from './E2ETestCase';

describe.each([
  { mode: 'tui' as const, builder: () => E2ETestCase.builder() },
  { mode: 'lite' as const, builder: () => E2ETestCase.builder().withLite() },
])('Validation error message propagation ($mode)', ({ mode, builder }) => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('surfaces service message for IMAGE_SIZE_EXCEEDED validation error', async () => {
    testCase = await builder()
      .withTestName(`validation-error-image-size-${mode}`)
      .launch();

    await testCase.waitForText('ask a question', 10000);
    await testCase.getSessionId();

    // Push a SendError that simulates the service returning a ValidationException
    // with reason=IMAGE_SIZE_EXCEEDED and a user-friendly message.
    // The Rust MockStreamItem::SendError variant is #[typeshare(skip)] but still
    // deserializable via serde.
    await testCase.pushSendMessageResponse([
      {
        kind: 'sendError',
        data: {
          kind: { Unknown: { reason_code: 'IMAGE_SIZE_EXCEEDED', message: 'Image exceeds maximum allowed size of 3.75MB' } },
          request_id: 'test-req-123',
          status_code: 400,
        },
      } as any,
    ]);

    // Send a user message to trigger the error
    await testCase.sendKeys('analyze this image');
    await testCase.sleepMs(100);
    await testCase.pressEnter();

    // Wait for the error to surface as a transient alert
    const store = await testCase.waitForStoreCondition(
      (s) => s.transientAlert !== null && s.transientAlert !== undefined,
      10000
    );

    console.log('Transient alert:', JSON.stringify(store.transientAlert));
    console.log('Snapshot:\n' + testCase.getSnapshotFormatted());

    expect(store.transientAlert).toBeTruthy();
    expect(store.transientAlert?.status).toBe('error');
    // The user should see the service's friendly message, not just the reason code
    expect(store.transientAlert?.message).toContain('Image exceeds maximum allowed size of 3.75MB');

    // Should NOT be processing anymore
    expect(store.isProcessing).toBe(false);
  }, 30000);
});
