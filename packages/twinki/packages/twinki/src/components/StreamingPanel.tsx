import React, { type ReactNode } from 'react';
import { Box } from './Box.js';

export type StreamingPanelProps = {
	readonly content: string;
	readonly streaming: boolean;
	readonly height: number;
	readonly scrollbar?: boolean;
	readonly scrollbarColor?: string;
	readonly onReadyToFlush?: () => void;
	readonly children: (content: string, scrollInfo: { scrollTop: number; totalLines: number; isScrolledUp: boolean }) => ReactNode;
};

export function StreamingPanel({ content, children }: StreamingPanelProps): React.ReactElement {
	const totalLines = content ? content.split('\n').length : 0;

	return (
		<Box flexDirection="column">
			{children(content, { scrollTop: 0, totalLines, isScrolledUp: false })}
		</Box>
	);
}
