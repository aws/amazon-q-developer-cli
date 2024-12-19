import { Packet } from "@aws/amazon-q-developer-cli-proto/mux"
import { Socket } from "./socket.js";

export class PacketReader {
    private reader: ReadableStream<string>
    private transformStream: TransformStream<string, Packet>

    constructor(socket: Socket) {
        this.reader = new ReadableStream({
            start(controller) {
                socket.onMessage((data) => {
                    if (typeof data === "string") {
                        controller.enqueue(data);
                    }
                });

                socket.onClose(() => controller.close());
            }
        })
        this.transformStream = new TransformStream(
            new StringParserTransform(parseBase64Line)
        );
    }

    getReader() {
        return this.reader.pipeThrough(this.transformStream)
    }
}

// Transform stream to handle parsing of incoming bytes
// Define possible parse results
type ParseSuccess<T> = {
    type: 'success';
    value: T;
    charsConsumed: number;
};

type ParseNeedsMore = {
    type: 'needs_more';
    minimumCharsNeeded: number;
};

type ParseError = {
    type: 'error';
    error: Error;
};

type ParseResult<T> = ParseSuccess<T> | ParseNeedsMore | ParseError;

class StringParserTransform<T> implements Transformer<string, T> {
    private buffer: string;

    constructor(
        private parse: (input: string) => ParseResult<T>
    ) {
        this.buffer = '';
    }

    transform(chunk: string, controller: TransformStreamDefaultController<T>) {
        // Decode and append new chunk to existing buffer
        this.buffer += chunk;

        // Keep trying to parse while we have data
        while (this.buffer.length > 0) {
            const result = this.parse(this.buffer);

            switch (result.type) {
                case 'success':
                    // Enqueue the parsed value and remove consumed characters
                    controller.enqueue(result.value);
                    this.buffer = this.buffer.slice(result.charsConsumed);
                    break;

                case 'needs_more':
                    // Not enough data yet, wait for more
                    if (result.minimumCharsNeeded > this.buffer.length) {
                        return;
                    }
                    // If we have enough chars but parse still failed, there might be an issue
                    throw new Error('Parser reported needs more characters but buffer contains requested amount');

                case 'error':
                    // Forward parse errors to stream consumer
                    controller.error(result.error);
                    return;
            }
        }
    }

    flush(controller: TransformStreamDefaultController<T>) {
        // Handle any remaining text
        if (this.buffer.length > 0) {
            const result = this.parse(this.buffer);
            if (result.type === 'success') {
                controller.enqueue(result.value);
            }
            // Ignore needs_more on flush since no more data is coming
            else if (result.type === 'error') {
                controller.error(result.error);
            }
        }
    }
}

function parseBase64Line(input: string): ParseResult<Packet> {
    // Look for either CRLF or LF
    const crlfIndex = input.indexOf('\r\n');
    const lfIndex = input.indexOf('\n');

    // Determine which line ending comes first (if any)
    let lineEndIndex: number;
    let lineEndLength: number;

    if (crlfIndex !== -1 && (lfIndex === -1 || crlfIndex < lfIndex)) {
        // CRLF comes first
        lineEndIndex = crlfIndex;
        lineEndLength = 2;
    } else if (lfIndex !== -1) {
        // LF comes first
        lineEndIndex = lfIndex;
        lineEndLength = 1;
    } else {
        // No line ending found yet
        return {
            type: 'needs_more',
            minimumCharsNeeded: input.length + 1
        };
    }

    // Extract the base64 string (excluding line ending)
    const base64Str = input.slice(0, lineEndIndex);

    // Validate base64 string
    if (!isValidBase64(base64Str)) {
        return {
            type: 'error',
            error: new Error('Invalid base64 string')
        };
    }

    try {
        // Decode base64 to Uint8Array
        const binaryStr = atob(base64Str);
        const bytes = new Uint8Array(binaryStr.length);
        for (let i = 0; i < binaryStr.length; i++) {
            bytes[i] = binaryStr.charCodeAt(i);
        }

        const packet = Packet.decode(bytes)

        return {
            type: 'success',
            value: packet,
            charsConsumed: lineEndIndex + lineEndLength
        };
    } catch (error) {
        return {
            type: 'error',
            error: new Error(`Failed to decode line: ${error}`)
        };
    }
}

// Helper function to validate base64 string
function isValidBase64(str: string): boolean {
    // Check if string matches base64 pattern
    // Allow padding at the end (=), must be multiple of 4 chars
    const base64Regex = /^[A-Za-z0-9+/]*={0,2}$/;
    return base64Regex.test(str) && str.length % 4 === 0;
}


