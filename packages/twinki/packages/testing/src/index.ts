export { VirtualTerminal } from './virtual-terminal.js';
export { FrameCapturingTerminal } from './frame-capturing-terminal.js';
export type { Frame } from './frame-capturing-terminal.js';
export { TestSession } from './test-session.js';
export { serializeFrame, serializeFrames, diffFrames } from './serializer.js';
export {
	analyzeFlicker,
	analyzeCollisions,
	measureTTR,
	measureInputLatency,
} from './analyzers/index.js';
export type {
	FlickerEvent,
	FlickerReport,
	CollisionEvent,
	CollisionReport,
	OverlayBounds,
	TTRResult,
	InputLatencyResult,
} from './analyzers/index.js';
