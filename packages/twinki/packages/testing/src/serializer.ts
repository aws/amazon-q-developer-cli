import type { Frame } from './frame-capturing-terminal.js';

export function serializeFrame(frame: Frame): string {
	const width = Math.max(...frame.viewport.map((l) => l.length), 20);
	const header = `Frame ${frame.index} (t=${Number(frame.timestamp / 1000000n)}ms, ${frame.writeBytes}B, ${frame.isFull ? 'full' : 'diff'}):`;
	const top = '┌' + '─'.repeat(width) + '┐';
	const bottom = '└' + '─'.repeat(width) + '┘';
	const lines = frame.viewport.map((l) => '│' + l.padEnd(width) + '│');
	return [header, top, ...lines, bottom].join('\n');
}

export function serializeFrames(frames: Frame[]): string {
	return frames.map(serializeFrame).join('\n\n');
}

export function diffFrames(a: Frame, b: Frame): string[] {
	const changed: string[] = [];
	const maxRows = Math.max(a.viewport.length, b.viewport.length);
	for (let i = 0; i < maxRows; i++) {
		const lineA = a.viewport[i] ?? '';
		const lineB = b.viewport[i] ?? '';
		if (lineA !== lineB) {
			changed.push(`row ${i}: ${JSON.stringify(lineA)} → ${JSON.stringify(lineB)}`);
		}
	}
	return changed;
}
