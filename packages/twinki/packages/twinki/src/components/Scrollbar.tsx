import React from 'react';
import { Box } from './Box.js';
import { Text } from './Text.js';

export type ScrollbarProps = {
	readonly scrollTop: number;
	readonly totalLines: number;
	readonly viewportHeight: number;
	readonly color?: string;
};

export function Scrollbar({ scrollTop, totalLines, viewportHeight, color }: ScrollbarProps): React.ReactElement | null {
	if (totalLines <= viewportHeight) return null;

	const trackHeight = Math.max(1, viewportHeight - 2);
	const thumbSize = Math.max(1, Math.round((viewportHeight / totalLines) * trackHeight));
	const maxScroll = totalLines - viewportHeight;
	const scrollRatio = maxScroll > 0 ? scrollTop / maxScroll : 0;
	const thumbPos = Math.round(scrollRatio * (trackHeight - thumbSize));

	const track = '▲\n' + Array.from({ length: trackHeight }, (_, i) =>
		i >= thumbPos && i < thumbPos + thumbSize ? '█' : '░'
	).join('\n') + '\n▼';

	return (
		<Box flexShrink={0} width={1}>
			<Text color={color ?? 'gray'}>{track}</Text>
		</Box>
	);
}
