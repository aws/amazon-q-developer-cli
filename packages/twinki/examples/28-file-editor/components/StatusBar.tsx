import React from 'react';
import { Box, Text } from 'twinki';
import { palette } from '../lib/palette.js';

export interface StatusBarProps {
	editing: boolean;
	fileName: string | null;
	language: string | undefined;
	dirty: boolean;
	theme: string;
	themeIndex: number;
	themeCount: number;
}

/**
 * NvChad-style statusline: a colored mode segment (NORMAL / INSERT) + the file
 * name and dirty marker on the left, filetype + theme on the right, and a
 * context-sensitive keybinding hint underneath.
 */
export const StatusBar: React.FC<StatusBarProps> = ({ editing, fileName, language, dirty, theme, themeIndex, themeCount }) => {
	const modeBg = editing ? palette.blue : palette.green;
	const keys = editing
		? 'Esc view   •   Ctrl+S save   •   type to edit'
		: '↑/↓ move   •   Enter open/fold   •   ←/→ collapse/expand   •   e edit   •   Tab/Shift+Tab theme   •   Ctrl+S save   •   q quit';

	return (
		<Box flexDirection="column">
			<Box flexDirection="row" justifyContent="space-between">
				<Box flexDirection="row">
					<Text color={palette.bg} backgroundColor={modeBg} bold>{editing ? ' INSERT ' : ' NORMAL '}</Text>
					<Text color={palette.fg} backgroundColor={palette.bgAlt}>{` ${fileName ?? 'no file'}${dirty ? ' ●' : ''} `}</Text>
				</Box>
				<Box flexDirection="row">
					<Text color={palette.fg} backgroundColor={palette.bgAlt}>{` ${language ?? 'text'} `}</Text>
					<Text color={palette.bg} backgroundColor={palette.purple} bold>{` ${theme}  ${themeIndex + 1}/${themeCount} `}</Text>
				</Box>
			</Box>
			<Box paddingX={1}>
				<Text color={palette.dim} wrap="truncate">{keys}</Text>
			</Box>
		</Box>
	);
};
