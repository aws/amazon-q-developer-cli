import type { Frame } from '../frame-capturing-terminal.js';

export interface TTRResult {
	ttr_ns: bigint;
	ttr_ms: number;
	frame: Frame;
}

export async function measureTTR(
	trigger: () => void,
	waitForFrame: (predicate: (frame: Frame) => boolean, timeoutMs?: number) => Promise<Frame>,
	predicate: (frame: Frame) => boolean,
	timeoutMs = 1000,
): Promise<TTRResult> {
	const start = process.hrtime.bigint();
	trigger();
	const frame = await waitForFrame(predicate, timeoutMs);
	const ttr_ns = frame.timestamp - start;
	return { ttr_ns, ttr_ms: Number(ttr_ns) / 1_000_000, frame };
}
