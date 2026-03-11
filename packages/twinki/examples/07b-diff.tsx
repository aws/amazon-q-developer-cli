/**
 * 07b-diff.tsx — DiffView component showcase
 *
 * Run: npx tsx examples/07b-diff.tsx
 *
 * Demonstrates:
 *   - layout="auto"       switches horizontal/vertical at 120 cols
 *   - layout="horizontal" side-by-side (VS Code split diff)
 *   - layout="vertical"   unified inline diff
 *   - highlight=true      shiki syntax highlighting
 *   - startLine metadata  line numbers from original file
 *
 * Controls:
 *   Tab       cycle through layouts (auto → horizontal → vertical)
 *   h         toggle syntax highlighting
 *   q / Ctrl+C  quit
 */
import React, { useState } from 'react';
import { render, Text, Box, DiffView, useInput, useApp, getHighlighter } from 'twinki';

const THEMES = ['github-dark', 'github-light', 'monokai', 'dracula', 'nord', 'tokyo-night', 'catppuccin-mocha', 'one-dark-pro'] as const;

// Preload all themes in background so switching is always synchronous
Promise.all(THEMES.map(t => getHighlighter(t, 'typescript'))).catch(() => {});

// ── Sample diff: a realistic TypeScript refactor ──────────────────────────────

const OLD_CODE = `import express from 'express';
import { db } from './db';

export async function getUser(req, res) {
  const id = req.params.id;
  const user = await db.query('SELECT * FROM users WHERE id = ' + id);
  if (!user) {
    res.status(404).send('Not found');
    return;
  }
  res.json(user);
}

export async function createUser(req, res) {
  const { name, email } = req.body;
  await db.query('INSERT INTO users (name, email) VALUES (?, ?)', [name, email]);
  res.status(201).send('Created');
}`;

const NEW_CODE = `import express from 'express';
import { db } from './db';
import { z } from 'zod';

const UserSchema = z.object({
  name: z.string().min(1),
  email: z.string().email(),
});

export async function getUser(req: Request, res: Response) {
  const id = parseInt(req.params.id, 10);
  if (isNaN(id)) return res.status(400).json({ error: 'Invalid id' });
  const user = await db.users.findUnique({ where: { id } });
  if (!user) {
    res.status(404).json({ error: 'Not found' });
    return;
  }
  res.json(user);
}

export async function createUser(req: Request, res: Response) {
  const parsed = UserSchema.safeParse(req.body);
  if (!parsed.success) return res.status(400).json(parsed.error);
  const user = await db.users.create({ data: parsed.data });
  res.status(201).json(user);
}`;

const LAYOUTS = ['auto', 'horizontal', 'vertical'] as const;

const App = () => {
	const [layoutIdx, setLayoutIdx] = useState(0);
	const [themeIdx, setThemeIdx] = useState(0);
	const [highlight, setHighlight] = useState(true);
	const { exit } = useApp();

	const layout = LAYOUTS[layoutIdx]!;
	const theme = THEMES[themeIdx]!;

	useInput((ch, key) => {
		if (ch === 'q' || key.ctrl && ch === 'c') exit();
		if (key.tab) setLayoutIdx(i => (i + 1) % LAYOUTS.length);
		if (ch === 't') setThemeIdx(i => (i + 1) % THEMES.length);
		if (ch === 'h') setHighlight(v => !v);
	});

	const cols = process.stdout.columns ?? 80;
	const resolvedLayout = layout === 'auto'
		? (cols >= 120 ? 'horizontal' : 'vertical')
		: layout;

	return (
		<Box flexDirection="column">
			{/* Header */}
			<Box borderStyle="round" borderColor="blue" paddingX={1} flexDirection="row">
				<Text>
					<Text bold color="blue">DiffView Demo  </Text>
					<Text dimColor>layout=</Text>
					<Text color="yellow" bold>{layout}</Text>
					<Text dimColor> → </Text>
					<Text color="cyan">{resolvedLayout}</Text>
					<Text dimColor>  theme=</Text>
					<Text color="magenta" bold>{theme}</Text>
					<Text dimColor>  highlight=</Text>
					<Text color={highlight ? 'green' : 'red'} bold>{String(highlight)}</Text>
				</Text>
			</Box>

			<Text> </Text>

			{/* The diff */}
			<DiffView
				values={[
					{ content: OLD_CODE, startLine: 1 },
					{ content: NEW_CODE, startLine: 1 },
				]}
				layout={layout}
				highlight={highlight}
				lang="typescript"
				theme={theme}
			/>

			<Text> </Text>

			{/* Controls */}
			<Text dimColor>  Tab cycle layout  •  t cycle theme  •  h toggle highlight  •  q quit</Text>
		</Box>
	);
};

render(<App />);
