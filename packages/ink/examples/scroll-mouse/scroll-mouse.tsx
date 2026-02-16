import React, {useState} from 'react';
import {render, Box, Text, useScroll, useMouse, useApp} from '../../src/index.js';

const items = Array.from({length: 30}, (_, i) => `Item ${i + 1}`);

function ScrollMouseDemo() {
	const {exit} = useApp();
	const {scrollTop, scrollBy} = useScroll();
	const [selected, setSelected] = useState<string | null>(null);

	const {ref} = useMouse({
		onClick: event => {
			const index = scrollTop + event.row - 2; // adjust for border+header
			if (index >= 0 && index < items.length) {
				setSelected(items[index]!);
			}
		},
		onRightClick: () => exit(),
	});

	return (
		<Box flexDirection="column">
			<Text>↑↓/PgUp/PgDn to scroll. Click item to select. Right-click to exit.</Text>
			<Box
				ref={ref}
				flexDirection="column"
				height={12}
				borderStyle="round"
				overflow="hidden"
				scrollTop={scrollTop}
			>
				{items.map(item => (
					<Text key={item} color={item === selected ? 'green' : undefined}>
						{item === selected ? '▸ ' : '  '}{item}
					</Text>
				))}
			</Box>
			{selected && <Text>Selected: <Text bold>{selected}</Text></Text>}
		</Box>
	);
}

render(<ScrollMouseDemo />);