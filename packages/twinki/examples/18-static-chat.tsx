/**
 * 18-static-chat.tsx — Chat with Static component for message history
 *
 * Run: npx tsx examples/18-static-chat.tsx
 *
 * Demonstrates the Static component for chat-like interfaces:
 *   - Completed messages move to Static (scrollback)
 *   - Only the current message stays in live area
 *   - Streaming responses word-by-word
 *   - Status bar shows render time metrics
 *   - Interactive text input
 *
 * Controls:
 *   Type a message and press Enter to send
 *   Ctrl+C to quit
 */
import React, { useState, useEffect, useRef } from 'react';
import { render, Text, Box, Static, Markdown, Typewriter, useInput, useApp, useTwinkiContext } from 'twinki';

interface Message {
	id: number;
	role: 'user' | 'assistant';
	content: string;
}

const LOREM = 'Lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua Ut enim ad minim veniam quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur Excepteur sint occaecat cupidatat non proident sunt in culpa qui officia deserunt mollit anim id est laborum';

function generateLorem(wordCount: number): string {
	const words = LOREM.split(' ');
	let result = '';
	for (let i = 0; i < wordCount; i++) {
		result += words[i % words.length] + ' ';
	}
	return result.trim();
}

const RESPONSES = [
	'Hello! How can I help you today?',
	`Here's a code example:\n\n\`\`\`typescript\nfunction greet(name: string): string {\n  return \`Hello, \${name}!\`;\n}\n\nconsole.log(greet('World'));\n\`\`\`\n\nThis demonstrates a simple TypeScript function.`,
	`The Static component is great for:\n\n- **Performance**: Completed messages don't re-render\n- **Scrollback**: Terminal handles history naturally\n- **Efficiency**: Only new content triggers updates\n\n> This is how chat apps should work!`,
	`**100 words:**\n\n${generateLorem(100)}`,
	`**500 words:**\n\n${generateLorem(500)}`,
	`**1000 words:**\n\n${generateLorem(1000)}`,
	`**2000 words:**\n\n${generateLorem(2000)}`,
];

const App = () => {
	const [messages, setMessages] = useState<Message[]>([]);
	const [input, setInput] = useState('');
	const [streaming, setStreaming] = useState<string | null>(null);
	const responseIdx = useRef(0);
	const { exit } = useApp();
	const { tui } = useTwinkiContext();

	const sendMessage = (text: string) => {
		if (streaming || !text.trim()) return;
		const userMsg: Message = {
			id: Date.now(),
			role: 'user',
			content: text.trim(),
		};
		setMessages(m => [...m, userMsg]);
		setInput('');
		const response = RESPONSES[responseIdx.current % RESPONSES.length]!;
		responseIdx.current++;
		setStreaming(response);
	};

	const onStreamComplete = () => {
		if (streaming) {
			setMessages(m => [...m, { id: Date.now(), role: 'assistant', content: streaming }]);
			setStreaming(null);
		}
	};

	useInput((ch, key) => {
		if (streaming !== null) return;
		if (key.return) {
			sendMessage(input);
			return;
		}
		if (key.backspace) {
			setInput(i => i.slice(0, -1));
			return;
		}
		if (ch && ch.length === 1 && ch.charCodeAt(0) >= 32) {
			setInput(i => i + ch);
		}
	});

	// Split messages: all but last go to Static
	const staticMessages = messages.slice(0, -1);
	const lastMessage = messages[messages.length - 1];

	// Header as first static item, then messages
	const staticItems = [
		{ id: 0, role: 'header' as const, content: '' },
		...staticMessages,
	];

	return (
		<Box flexDirection="column">
			{/* Static: header + completed messages (scrollback) */}
			<Static items={staticItems}>
				{(item: Message | { id: number; role: 'header'; content: string }) => (
					item.role === 'header' ? (
						<Box key={item.id} flexDirection="column">
							<Box borderStyle="round" borderColor="cyan">
								<Text bold color="cyan"> Static Chat Demo </Text>
							</Box>
							<Text> </Text>
						</Box>
					) : (
						<Box key={item.id} flexDirection="column">
							<Text color={item.role === 'user' ? 'cyan' : 'green'} bold>
								{item.role === 'user' ? '  You:' : '  Assistant:'}
							</Text>
							{item.role === 'user' ? (
								<Text>  {item.content}</Text>
							) : (
								<Box paddingLeft={2}><Markdown>{item.content}</Markdown></Box>
							)}
							<Text> </Text>
						</Box>
					)
				)}
			</Static>

			{/* Live: current message */}
			{lastMessage && streaming === null && (
				<Box flexDirection="column">
					<Text color={lastMessage.role === 'user' ? 'cyan' : 'green'} bold>
						{lastMessage.role === 'user' ? '  You:' : '  Assistant:'}
					</Text>
					{lastMessage.role === 'user' ? (
						<Text>  {lastMessage.content}</Text>
					) : (
						<Box paddingLeft={2}><Markdown>{lastMessage.content}</Markdown></Box>
					)}
					<Text> </Text>
				</Box>
			)}

			{/* Streaming response */}
			{streaming !== null && (
				<Box flexDirection="column">
					<Text color="green" bold>  Assistant:</Text>
					<Box paddingLeft={2}>
						<Typewriter speed="fast" onComplete={onStreamComplete}>{streaming}</Typewriter>
					</Box>
					<Text> </Text>
				</Box>
			)}

			{streaming === null && (
				<Box flexDirection="column">
					<Text dimColor>{'─'.repeat(50)}</Text>
					<Text>  <Text color="cyan" bold>{'>'}</Text> {input}<Text dimColor>│</Text></Text>
				</Box>
			)}

			<Text> </Text>
			<Text dimColor>
				  Messages: {messages.length}  •  Render: {tui.perfLastRenderMs.toFixed(2)}ms  •  Ctrl+C to quit
			</Text>
		</Box>
	);
};

render(<App />);
