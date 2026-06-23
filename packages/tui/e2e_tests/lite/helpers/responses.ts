import type { E2ETestCase } from '../../E2ETestCase';
import type { MockStreamItem } from '../../types/chat-cli';
import { sendUserMessage } from './commands';

/** Wrap content as a streaming AssistantResponseEvent mock item. */
export function assistantEvent(content: string): MockStreamItem {
  return {
    kind: 'event',
    data: { kind: 'AssistantResponseEvent', data: { content } },
  };
}

/**
 * Push one (or several) assistant-response chunks, then close the stream.
 * Pass `keepOpen: true` to stream more before sending the terminating null
 * yourself (e.g. when injecting input between chunks).
 */
export async function streamReply(
  tc: E2ETestCase,
  contents: string | string[],
  opts: { keepOpen?: boolean; silent?: boolean } = {}
): Promise<void> {
  const list = Array.isArray(contents) ? contents : [contents];
  await tc.pushSendMessageResponse(list.map(assistantEvent), {
    silent: opts.silent,
  });
  if (!opts.keepOpen)
    await tc.pushSendMessageResponse(null, { silent: opts.silent });
}

/** Drive one full e2e turn: queue `reply`, submit `prompt`, wait for the reply to paint, then settle. */
export async function driveTurn(
  tc: E2ETestCase,
  reply: string,
  prompt: string,
  opts: { waitIdle?: boolean } = {}
): Promise<void> {
  await streamReply(tc, reply);
  await sendUserMessage(tc, prompt);
  await tc.waitForText(reply, 15000);
  if (opts.waitIdle !== false) await tc.waitForIdle(10000);
}

/** Flatten all store messages to a single searchable string. */
export async function messageText(tc: E2ETestCase): Promise<string> {
  const store = await tc.getStore();
  return store.messages.map((m: unknown) => JSON.stringify(m)).join(' ');
}
