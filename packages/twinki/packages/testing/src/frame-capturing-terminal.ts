import { VirtualTerminal } from './virtual-terminal.js';

export interface Frame {
	index: number;
	timestamp: bigint;
	viewport: string[];
	scrollBuffer: string[];
	cursor: { x: number; y: number };
	writeBytes: number;
	isFull: boolean;
}

export class FrameCapturingTerminal extends VirtualTerminal {
	private frames: Frame[] = [];
	private frameIndex = 0;
	private pendingCapture = false;
	private pendingBytes = 0;
	private pendingIsFull = false;

	write(data: string): void {
		super.write(data);

		// Detect end of synchronized output as frame boundary
		if (data.includes('\x1b[?2026l')) {
			this.pendingCapture = true;
			this.pendingBytes = data.length;
			this.pendingIsFull = data.includes('\x1b[3J');
		}
	}

	async flush(): Promise<void> {
		await super.flush();
		if (this.pendingCapture) {
			this.frames.push({
				index: this.frameIndex++,
				timestamp: process.hrtime.bigint(),
				viewport: this.getViewport(),
				scrollBuffer: this.getScrollBuffer(),
				cursor: this.getCursorPosition(),
				writeBytes: this.pendingBytes,
				isFull: this.pendingIsFull,
			});
			this.pendingCapture = false;
			this.pendingBytes = 0;
			this.pendingIsFull = false;
		}
	}

	getFrames(): Frame[] {
		return [...this.frames];
	}

	getLastFrame(): Frame | undefined {
		return this.frames[this.frames.length - 1];
	}

	clearFrames(): void {
		this.frames = [];
		this.frameIndex = 0;
	}
}
