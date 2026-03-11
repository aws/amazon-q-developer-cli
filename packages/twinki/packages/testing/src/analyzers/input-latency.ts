import type { Frame } from '../frame-capturing-terminal.js';

export interface InputLatencyResult {
	latency_ns: bigint;
	latency_ms: number;
	frame: Frame;
}

export async function measureInputLatency(
	sendKey: () => void,
	waitForFrame: (predicate: (frame: Frame) => boolean, timeoutMs?: number) => Promise<Frame>,
	predicate: (frame: Frame) => boolean,
	timeoutMs = 1000,
): Promise<InputLatencyResult> {
	const start = process.hrtime.bigint();
	sendKey();
	const frame = await waitForFrame(predicate, timeoutMs);
	const latency_ns = frame.timestamp - start;
	return { latency_ns, latency_ms: Number(latency_ns) / 1_000_000, frame };
}
