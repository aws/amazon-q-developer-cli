/**
 * 04-todo.tsx — Interactive todo list
 *
 * Run: npx tsx examples/04-todo.tsx
 *
 * Controls:
 *   ↑/↓      Navigate
 *   space     Toggle done
 *   a         Add new item
 *   d         Delete selected
 *   q         Quit
 */
import React, { useState } from 'react';
import { render, Text, Box, useInput, useApp } from 'twinki';

interface Todo {
	id: number;
	text: string;
	done: boolean;
}

const INITIAL: Todo[] = [
	{ id: 1, text: 'Set up Twinki project', done: true },
	{ id: 2, text: 'Implement rendering engine', done: true },
	{ id: 3, text: 'Write E2E tests', done: true },
	{ id: 4, text: 'Create sample apps', done: false },
	{ id: 5, text: 'Publish to npm', done: false },
];

const TodoApp = () => {
	const [todos, setTodos] = useState<Todo[]>(INITIAL);
	const [cursor, setCursor] = useState(0);
	const [nextId, setNextId] = useState(6);
	const { exit } = useApp();

	useInput((input, key) => {
		if (input === 'q') { exit(); return; }
		if (key.upArrow) setCursor(c => Math.max(0, c - 1));
		if (key.downArrow) setCursor(c => Math.min(todos.length - 1, c + 1));
		if (input === ' ') {
			setTodos(t => t.map((item, i) =>
				i === cursor ? { ...item, done: !item.done } : item
			));
		}
		if (input === 'd' && todos.length > 0) {
			setTodos(t => t.filter((_, i) => i !== cursor));
			setCursor(c => Math.min(c, Math.max(0, todos.length - 2)));
		}
		if (input === 'a') {
			setTodos(t => [...t, { id: nextId, text: `New task ${nextId}`, done: false }]);
			setNextId(n => n + 1);
			setCursor(todos.length);
		}
	});

	const doneCount = todos.filter(t => t.done).length;

	return (
		<Box flexDirection="column">
			<Text bold>Todo List</Text>
			<Text dimColor>  {doneCount}/{todos.length} completed</Text>
			<Text> </Text>
			{todos.map((todo, i) => {
				const selected = i === cursor;
				const check = todo.done ? '✓' : '○';
				const checkColor = todo.done ? 'green' : 'gray';
				return (
					<Text key={todo.id}>
						<Text color={selected ? 'cyan' : undefined}>{selected ? '❯' : ' '}</Text>
						{' '}
						<Text color={checkColor}>{check}</Text>
						{' '}
						<Text strikethrough={todo.done} dimColor={todo.done}>{todo.text}</Text>
					</Text>
				);
			})}
			{todos.length === 0 && <Text dimColor>  No todos. Press 'a' to add one.</Text>}
			<Text> </Text>
			<Text dimColor>  ↑↓ navigate  space toggle  a add  d delete  q quit</Text>
		</Box>
	);
};

render(<TodoApp />);
