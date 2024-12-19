// import { beforeEach, describe, expect, it, vi } from "vitest";
// import { MockWebSocket } from "./websocket.test.js";
// import { PacketWriter } from "./writer.js";
// import { Packet, Packet_Compression } from "@aws/amazon-q-developer-cli-proto/mux";

import { describe, it } from "vitest";

it("null", () => { })

// describe('WebSocket Writer', () => {
//     let mockWs: MockWebSocket;
//     let encoder: TextEncoder;
//     let decoder: TextDecoder;
// 
//     beforeEach(() => {
//         mockWs = new MockWebSocket('ws://test');
//         encoder = new TextEncoder();
//         decoder = new TextDecoder();
// 
//         // Add send method to mock
//         (mockWs as any).send = vi.fn();
// 
//     });
// 
//     describe('Base64Writer', () => {
//         it('should write a simple message as base64 with newline', async () => {
//             const writer = await new PacketWriter(mockWs as unknown as WebSocket);
//             const message = {
//                 compression: Packet_Compression.NONE,
//                 inner: new Uint8Array(),
//                 nonce: new Uint8Array(),
//                 version: 0
//             } as Packet;
// 
//             (await writer.getWriter()).write(message);
// 
//             expect(mockWs.send).toHaveBeenCalledTimes(1);
//             const sentBytes = (mockWs.send as any).mock.calls[0][0];
//             const sentText = decoder.decode(sentBytes);
//             expect(sentText).toBe('aGVsbG8=\n');
//         });
// 
//         it('should handle empty messages', async () => {
//             const writer = await setupWebSocketWriter(mockWs as unknown as WebSocket);
//             const message = new Uint8Array([]); // empty
// 
//             await writer.getWriter().write(message);
// 
//             expect(mockWs.send).toHaveBeenCalledTimes(1);
//             const sentBytes = (mockWs.send as any).mock.calls[0][0];
//             const sentText = decoder.decode(sentBytes);
//             expect(sentText).toBe('\n');
//         });
// 
//         it('should handle messages with null bytes', async () => {
//             const writer = await setupWebSocketWriter(mockWs as unknown as WebSocket);
//             const message = new Uint8Array([0, 255, 0]); // bytes with nulls
// 
//             await writer.getWriter().write(message);
// 
//             expect(mockWs.send).toHaveBeenCalledTimes(1);
//             const sentBytes = (mockWs.send as any).mock.calls[0][0];
//             const sentText = decoder.decode(sentBytes);
//             expect(sentText).toBe('AP8A\n');
//         });
// 
//         it('should handle multiple writes', async () => {
//             const writer = await setupWebSocketWriter(mockWs as unknown as WebSocket);
//             const writerInstance = writer.getWriter();
// 
//             await writerInstance.write(new Uint8Array([104, 101, 108, 108, 111])); // "hello"
//             await writerInstance.write(new Uint8Array([119, 111, 114, 108, 100])); // "world"
// 
//             expect(mockWs.send).toHaveBeenCalledTimes(2);
//             const firstSentBytes = (mockWs.send as any).mock.calls[0][0];
//             const secondSentBytes = (mockWs.send as any).mock.calls[1][0];
// 
//             expect(decoder.decode(firstSentBytes)).toBe('aGVsbG8=\n');
//             expect(decoder.decode(secondSentBytes)).toBe('d29ybGQ=\n');
//         });
// 
//         it('should close properly', async () => {
//             const writer = await setupWebSocketWriter(mockWs as unknown as WebSocket);
//             const writerInstance = writer.getWriter();
// 
//             await writerInstance.write(new Uint8Array([104, 101, 108, 108, 111]));
//             await writerInstance.close();
// 
//             // You might want to add expectations about WebSocket closing here
//             // depending on your implementation
//         });
//     });
// 
//     describe('Two-way communication', () => {
//         it('should handle bidirectional communication', async () => {
//             const streams = setupWebSocketStreams(mockWs as unknown as WebSocket);
//             const reader = streams.readable.getReader();
//             const writer = streams.writable.getWriter();
// 
//             // Write a message
//             await writer.write(new Uint8Array([104, 101, 108, 108, 111])); // "hello"
// 
//             // Simulate receiving the same message back
//             mockWs.emit('message', {
//                 data: encoder.encode('aGVsbG8=\n').buffer
//             });
// 
//             // Read the received message
//             const { value, done } = await reader.read();
//             expect(done).toBe(false);
//             expect(new TextDecoder().decode(value)).toBe('hello');
// 
//             // Verify the sent message
//             expect(mockWs.send).toHaveBeenCalledTimes(1);
//             const sentBytes = (mockWs.send as any).mock.calls[0][0];
//             const sentText = decoder.decode(sentBytes);
//             expect(sentText).toBe('aGVsbG8=\n');
//         });
//     });
// });