import type { E2ETestCase } from '../../E2ETestCase';
import type { MockStreamItem } from '../../types/chat-cli';

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
