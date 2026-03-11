import type { Frame } from '../frame-capturing-terminal.js';

export interface CollisionEvent {
	frameIndex: number;
	row: number;
	col: number;
	overlayIndex: number;
}

export interface CollisionReport {
	events: CollisionEvent[];
	clean: boolean;
}

export interface OverlayBounds {
	row: number;
	col: number;
	width: number;
	height: number;
}

export function analyzeCollisions(frames: Frame[], overlayBounds: OverlayBounds[]): CollisionReport {
	const events: CollisionEvent[] = [];

	for (const frame of frames) {
		for (let oi = 0; oi < overlayBounds.length; oi++) {
			const bounds = overlayBounds[oi]!;
			for (let row = bounds.row; row < bounds.row + bounds.height; row++) {
				const line = frame.viewport[row];
				if (!line) continue;
				for (let col = bounds.col; col < bounds.col + bounds.width; col++) {
					if (col >= line.length) continue;
					// Check if content extends beyond declared bounds
					if (col < bounds.col || col >= bounds.col + bounds.width ||
						row < bounds.row || row >= bounds.row + bounds.height) {
						events.push({ frameIndex: frame.index, row, col, overlayIndex: oi });
					}
				}
			}
		}
	}

	return { events, clean: events.length === 0 };
}
