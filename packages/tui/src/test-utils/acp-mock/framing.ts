/**
 * Line-delimited JSON framing helpers for ACP mock transport.
 *
 * The mock transport multiplexes JSON-RPC traffic over a Unix socket using
 * the same newline-delimited JSON convention the ACP SDK uses for stdio.
 * These helpers provide buffered encode / decode without any protocol
 * awareness - a message is just an arbitrary JSON value.
 */

export function encodeFrame(value: unknown): string {
  // Throws on circular references. Caller surfaces.
  return JSON.stringify(value) + '\n';
}

/**
 * Stateful decoder. Feed chunks as they arrive from the socket; `drain()`
 * yields complete messages and preserves any trailing partial line.
 */
export class FrameDecoder {
  private buffer = '';

  /**
   * Appends `chunk` to the internal buffer and returns any complete
   * messages it now contains. Malformed JSON is reported via `onError`
   * and skipped; the decoder continues with the next line.
   */
  feed(
    chunk: string,
    onError?: (line: string, err: unknown) => void
  ): unknown[] {
    this.buffer += chunk;
    const lines = this.buffer.split('\n');
    // Last element is either empty (if chunk ended with \n) or a partial line.
    this.buffer = lines.pop() ?? '';
    const out: unknown[] = [];
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) continue;
      try {
        out.push(JSON.parse(trimmed));
      } catch (err) {
        if (onError) onError(trimmed, err);
      }
    }
    return out;
  }

  /**
   * Returns any final message left in the buffer (when the connection
   * closes cleanly and the last message was not newline-terminated).
   * After calling this, the decoder is empty.
   */
  finalize(onError?: (line: string, err: unknown) => void): unknown[] {
    const remaining = this.buffer.trim();
    this.buffer = '';
    if (!remaining) return [];
    try {
      return [JSON.parse(remaining)];
    } catch (err) {
      if (onError) onError(remaining, err);
      return [];
    }
  }
}
