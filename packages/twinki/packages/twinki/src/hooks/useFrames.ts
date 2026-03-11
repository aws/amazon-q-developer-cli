import { useState, useEffect } from 'react';

/**
 * Returns a frame counter that increments at the given FPS.
 * Useful for driving sprite animations and time-based effects.
 */
export function useFrames(fps = 12): number {
	const [frame, setFrame] = useState(0);
	useEffect(() => {
		const id = setInterval(() => setFrame(f => f + 1), 1000 / fps);
		return () => clearInterval(id);
	}, [fps]);
	return frame;
}
