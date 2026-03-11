/**
 * 06-chat.tsx — AI Chat Interface
 *
 * Run: npx tsx examples/06-chat.tsx
 *
 * A realistic chat app demonstrating:
 *   - Scrolling message history
 *   - Simulated AI streaming responses
 *   - Typing indicator animation
 *   - Status bar at the bottom
 *   - Input handling
 *
 * Controls:
 *   Type a message and press Enter to send
 *   Ctrl+C to quit
 */
import React, { useState, useEffect, useCallback, useRef } from 'react';
import { render, Text, Box, Markdown, useInput, useApp } from 'twinki';

interface Message {
	role: 'user' | 'assistant' | 'system';
	content: string;
}

const RESPONSES = [
	"That's a great question! Let me think about it...\n\nThe answer involves several key concepts:\n\n1. First, we need to understand the fundamentals — every system has a core abstraction that everything else builds on.\n\n2. Then we can build on that foundation by layering increasingly specific behaviors.\n\n3. Finally, we arrive at the solution by composing these layers together.\n\nThe key insight is that complexity emerges from simple rules applied consistently.",

	"Here's what I know about that:\n\nTerminal rendering is fundamentally a text-based protocol dating back to the VT100 in 1978. Modern terminals still speak this protocol, but with extensions:\n\n- **SGR codes** for colors and styles (bold, italic, underline)\n- **CSI sequences** for cursor movement and screen clearing\n- **OSC sequences** for hyperlinks and window titles\n- **Kitty protocol** for precise key press/release reporting\n\nThe challenge is that every terminal implements these slightly differently. Twinki handles this by detecting capabilities at startup and adapting its output accordingly.",

	"I'd be happy to help with that! Here's a step-by-step approach:\n\n```\n$ npm init -y\n$ npm install twinki react\n$ mkdir src && touch src/app.tsx\n```\n\nThen in `src/app.tsx`:\n\n```tsx\nimport { render, Text, Box } from 'twinki';\n\nconst App = () => (\n  <Box flexDirection=\"column\">\n    <Text bold>My CLI Tool</Text>\n    <Text>Hello from Twinki!</Text>\n  </Box>\n);\n\nrender(<App />);\n```\n\nRun it with `npx tsx src/app.tsx` and you should see your app rendered inline in the terminal — no alternate screen, no flicker.",

	"Interesting question! The difference between Ink and Twinki comes down to rendering strategy:\n\n**Ink** repaints the entire screen every frame (16ms timer). This causes flicker because the terminal briefly shows a blank screen between frames. It also uses alternate screen mode, which destroys your scrollback history.\n\n**Twinki** uses differential rendering — it compares the new frame against the previous one and only rewrites the lines that actually changed. Every frame is wrapped in synchronized output markers so the terminal displays it atomically.\n\nThe result: zero flicker, preserved scrollback, and sub-millisecond frame times for typical updates.",

	"Sure thing! Let me explain how the rendering pipeline works:\n\n```\nReact setState()\n  → reconciler schedules update\n  → Yoga calculates layout\n  → TUI.render() collects lines from component tree\n  → line-by-line diff against previousLines[]\n  → build single escape sequence buffer\n  → wrap in synchronized output (CSI ?2026h/l)\n  → ONE terminal.write() call\n```\n\nThe critical insight is that `process.nextTick` debouncing means multiple state changes within the same tick coalesce into a single render. So even if you call `setState` 10 times in a row, only one frame gets written to the terminal.\n\nThis is why Twinki can handle rapid updates (like streaming AI responses) without any flicker — each chunk of text triggers a state change, but they all merge into minimal differential updates.",

	"Great observation! The testing framework is one of Twinki's unique features.\n\nInstead of snapshot testing (which only captures text), Twinki captures actual terminal frames using `@xterm/headless` — a real terminal emulator running in Node.js. This means:\n\n- **Frame capture**: Every synchronized output boundary creates a frame with the full viewport state\n- **Flicker detection**: The analyzer checks consecutive frame triples for non-blank → blank → non-blank patterns\n- **Differential verification**: You can assert that `frame.isFull === false` to prove only changed lines were rewritten\n- **Byte efficiency**: Compare `frame.writeBytes` between first render and updates\n\nThis catches bugs that text snapshots miss — like ANSI code leaking between lines, or cursor positioning errors that cause visual glitches.",
];

const SPINNERS = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const MessageView = ({ msg }: { msg: Message }) => {
	if (msg.role === 'system') {
		return <Text dimColor italic>  {msg.content}</Text>;
	}
	if (msg.role === 'user') {
		return (
			<Box flexDirection="column">
				<Text color="cyan" bold>  You:</Text>
				<Text>  {msg.content}</Text>
			</Box>
		);
	}
	// assistant — render markdown with syntax highlighting
	return (
		<Box flexDirection="column">
			<Text color="green" bold>  AI:</Text>
			<Box paddingLeft={2}>
				<Markdown>{msg.content}</Markdown>
			</Box>
		</Box>
	);
};

/** Close any unclosed code fences so partial markdown parses safely */
function closeOpenFences(text: string): string {
	const fenceCount = (text.match(/^```/gm) || []).length;
	return fenceCount % 2 === 1 ? text + '\n```' : text;
}

const ChatApp = () => {
	const [messages, setMessages] = useState<Message[]>([
		{ role: 'system', content: 'Welcome to Twinki Chat! Type a message and press Enter.' },
	]);
	const [input, setInput] = useState('');
	const [streaming, setStreaming] = useState(false);
	const [streamText, setStreamText] = useState('');
	const [spinFrame, setSpinFrame] = useState(0);
	const [status, setStatus] = useState('Ready');
	const responseIdx = useRef(0);
	const { exit } = useApp();

	// Spinner animation during streaming
	useEffect(() => {
		if (!streaming) return;
		const timer = setInterval(() => setSpinFrame(f => f + 1), 80);
		return () => clearInterval(timer);
	}, [streaming]);

	// Simulate streaming response
	const simulateResponse = useCallback(() => {
		const fullResponse = RESPONSES[responseIdx.current % RESPONSES.length]!;
		responseIdx.current++;
		const words = fullResponse.split(/(?<=\s)/); // split keeping whitespace
		let idx = 0;

		setStreaming(true);
		setStreamText('');
		setStatus('AI is responding...');

		const timer = setInterval(() => {
			if (idx >= words.length) {
				clearInterval(timer);
				setStreaming(false);
				setMessages(m => [...m, { role: 'assistant', content: fullResponse }]);
				setStreamText('');
				setStatus('Ready');
				return;
			}
			setStreamText(t => t + words[idx]!);
			idx++;
		}, 30 + Math.random() * 40); // 30-70ms per word, feels natural
	}, []);

	useInput((ch, key) => {
		if (streaming) return;

		if (key.return && input.trim()) {
			const userMsg = input.trim();
			setMessages(m => [...m, { role: 'user', content: userMsg }]);
			setInput('');
			setStatus('Sending...');
			setTimeout(() => simulateResponse(), 300);
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

	const spinner = SPINNERS[spinFrame % SPINNERS.length];

	return (
		<Box flexDirection="column">
			{/* Header */}
			<Box borderStyle="round" borderColor="cyan">
				<Text bold color="cyan"> Twinki Chat </Text>
			</Box>

			<Text> </Text>

			{/* Message history */}
			{messages.map((msg, i) => (
				<Box key={i} flexDirection="column">
					<MessageView msg={msg} />
					<Text> </Text>
				</Box>
			))}

			{/* Streaming response — render as markdown, closing any open fences */}
			{streaming && streamText && (
				<Box flexDirection="column">
					<Text color="green" bold>  AI:</Text>
					<Box paddingLeft={2}>
						<Markdown>{closeOpenFences(streamText)}</Markdown>
					</Box>
					<Text> </Text>
				</Box>
			)}

			{/* Typing indicator */}
			{streaming && !streamText && (
				<Text>  <Text color="yellow">{spinner}</Text> AI is thinking...</Text>
			)}

			{/* Input area */}
			{!streaming && (
				<Box flexDirection="column">
					<Text>{'─'.repeat(50)}</Text>
					<Text>  <Text color="cyan" bold>{'>'}</Text> {input}<Text color="gray">│</Text></Text>
				</Box>
			)}

			{/* Status bar */}
			<Text> </Text>
			<Text dimColor>  {status}  •  {messages.filter(m => m.role !== 'system').length} messages  •  Ctrl+C to quit</Text>
		</Box>
	);
};

render(<ChatApp />);
