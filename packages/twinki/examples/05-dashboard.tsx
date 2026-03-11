/**
 * 05-dashboard.tsx — Multi-section system dashboard
 *
 * Run: npx tsx examples/05-dashboard.tsx
 *
 * Demonstrates:
 *   - Multiple layout sections (header, body, footer)
 *   - Timer-driven data updates
 *   - Differential rendering (only changed values update)
 *   - Box borders and padding
 */
import React, { useState, useEffect } from 'react';
import { render, Text, Box, useInput, useApp } from 'twinki';

function formatUptime(seconds: number): string {
	const h = Math.floor(seconds / 3600);
	const m = Math.floor((seconds % 3600) / 60);
	const s = seconds % 60;
	return `${h}h ${m}m ${s}s`;
}

function randomBetween(min: number, max: number): number {
	return Math.floor(Math.random() * (max - min + 1)) + min;
}

const Dashboard = () => {
	const [uptime, setUptime] = useState(0);
	const [cpu, setCpu] = useState(23);
	const [mem, setMem] = useState(41);
	const [reqs, setReqs] = useState(1247);
	const [errors, setErrors] = useState(3);
	const [status, setStatus] = useState<'healthy' | 'degraded' | 'down'>('healthy');
	const { exit } = useApp();

	useInput((input) => {
		if (input === 'q') exit();
	});

	useEffect(() => {
		const timer = setInterval(() => {
			setUptime(u => u + 1);
			setCpu(randomBetween(15, 85));
			setMem(randomBetween(35, 70));
			setReqs(r => r + randomBetween(1, 20));
			if (Math.random() < 0.05) setErrors(e => e + 1);
			setStatus(cpu > 80 ? 'degraded' : 'healthy');
		}, 1000);
		return () => clearInterval(timer);
	}, [cpu]);

	const statusColor = status === 'healthy' ? 'green' : status === 'degraded' ? 'yellow' : 'red';
	const cpuColor = cpu > 70 ? 'red' : cpu > 50 ? 'yellow' : 'green';
	const cpuBar = '█'.repeat(Math.floor(cpu / 5)) + '░'.repeat(20 - Math.floor(cpu / 5));

	return (
		<Box flexDirection="column">
			<Box borderStyle="round" borderColor="cyan" padding={1} alignItems="center">
				<Text bold color="cyan">System Dashboard</Text>
			</Box>

			<Text> </Text>

			<Box flexDirection="column" padding={1}>
				<Text>  Status:  <Text color={statusColor} bold>{status.toUpperCase()}</Text></Text>
				<Text>  Uptime:  {formatUptime(uptime)}</Text>
				<Text> </Text>
				<Text>  CPU:     <Text color={cpuColor}>{cpuBar}</Text> {cpu}%</Text>
				<Text>  Memory:  {mem}%</Text>
				<Text>  Requests: {reqs.toLocaleString()}</Text>
				<Text>  Errors:  <Text color={errors > 5 ? 'red' : 'gray'}>{errors}</Text></Text>
			</Box>

			<Text> </Text>
			<Text dimColor>  Press q to quit  •  Updates every 1s</Text>
		</Box>
	);
};

render(<Dashboard />);
