import { useEffect, useState } from 'react';
import { useTwinkiContext } from 'twinki';

/** Terminal dimensions in columns/rows. */
export interface TerminalSize {
	columns: number;
	rows: number;
}

/**
 * Reads the render terminal's dimensions and updates on resize. Uses the TUI's
 * terminal (via context) rather than process.stdout, so it works under the
 * headless test harness too.
 */
export function useTerminalSize(): TerminalSize {
	const { tui } = useTwinkiContext();
	const [size, setSize] = useState<TerminalSize>({
		columns: tui.terminal.columns,
		rows: tui.terminal.rows,
	});

	useEffect(() => {
		const update = () => setSize({ columns: tui.terminal.columns, rows: tui.terminal.rows });
		update();
		// TUI keeps a list of resize callbacks; the root App lives for the whole
		// session, so we don't need to unsubscribe.
		tui.onResize(update);
	}, [tui]);

	return size;
}
