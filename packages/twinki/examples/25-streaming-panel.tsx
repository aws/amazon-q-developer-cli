/**
 * 25-streaming-panel.tsx — StreamingPanel with Markdown + StatusBar
 *
 * Run: npx tsx examples/25-streaming-panel.tsx
 *
 * Simulates the real TUI: StreamingPanel → StatusBar → Markdown content.
 * Press Space to grow content beyond viewport.
 *
 * Controls:
 *   Space    — add more markdown content
 *   Ctrl+C   — quit
 */
import React, { useState } from 'react';
import { render, Text, Box, Markdown, useInput, useApp, StreamingPanel } from 'twinki';

const CHROME = 6;

const StatusBar: React.FC<{ children: React.ReactNode; lineCount: number; color?: string }> = ({ children, lineCount, color = 'blue' }) => (
	<Box flexDirection="row">
		<Box flexDirection="column" width={1}>
			{Array.from({ length: lineCount }, (_, i) => (
				<Text key={i} backgroundColor={color}>{' '}</Text>
			))}
		</Box>
		<Box flexDirection="column" flexGrow={1} marginLeft={1}>
			{children}
		</Box>
	</Box>
);

const CHUNKS = [
	'## DynamoDB Overview\n\nFully managed **NoSQL** database service by AWS.',
	'\n\n- Single-digit millisecond performance at any scale\n- Supports key-value and document data models\n- Automatic scaling with on-demand mode',
	'\n\n## Capacity Modes\n\n| Mode | Description | Use Case |\n|------|-------------|----------|\n| On-Demand | Pay per request | Unpredictable traffic |\n| Provisioned | Set RCU/WCU | Predictable workloads |',
	'\n\n## Indexes\n\n**Local Secondary Index (LSI)**\n- Same partition key, different sort key\n- Must be defined at table creation\n- Max 5 per table\n- Shares capacity with base table\n- Supports strongly consistent reads',
	'\n\n**Global Secondary Index (GSI)**\n- Different partition + sort key\n- Can be added anytime\n- Max 20 per table\n- Only eventually consistent reads\n- Has its own provisioned capacity\n- Useful for inverting access patterns',
	'\n\nGSI projection types:\n- `KEYS_ONLY` — only PK, SK, and index keys\n- `INCLUDE` — keys plus specified attributes\n- `ALL` — all attributes (most storage)',
	'\n\n───────────────────────────────────────────────\n\n## Transactions\n\n- `TransactWriteItems` — up to 100 put/update/delete/condition-check operations atomically\n- `TransactGetItems` — up to 100 strongly consistent reads atomically\n- All-or-nothing: if any operation fails, entire transaction is rolled back\n- Costs 2x RCUs/WCUs compared to non-transactional operations\n- Cannot span multiple AWS accounts or regions',
	'\n\n## Streams\n\n- Captures a time-ordered sequence of item-level changes\n- Retention: 24 hours\n- Stream record contains: keys only, new image, old image, or both images\n- Exactly-once delivery per shard\n- Integrates natively with Lambda (event source mapping)\n- Use cases: replication, audit logs, cache invalidation, event-driven workflows',
	'\n\n**Kinesis Data Streams alternative**\n- Longer retention (up to 1 year)\n- Higher throughput (no shard limits from DynamoDB side)\n- Supports Kinesis Consumer Library, Firehose, Analytics',
	'\n\n───────────────────────────────────────────────\n\n## Global Tables\n\n- Multi-region, multi-active (read and write in any region)\n- DynamoDB handles replication automatically\n- Conflict resolution: last-writer-wins based on timestamp\n- Each region is a full replica — reads are local latency\n- Requires Streams enabled (uses them internally)\n- Version 2019.11.21 supports adding regions to existing tables',
	'\n\n## DAX (DynamoDB Accelerator)\n\n- In-memory cache for DynamoDB\n- **Microsecond** read latency for cached items\n- Write-through caching — writes go to DynamoDB, reads served from cache\n- API-compatible with DynamoDB — just change the endpoint\n- Cluster-based: 1-11 nodes, multi-AZ\n- Not suitable for strongly consistent reads or write-heavy workloads',
	'\n\n```python\nimport boto3\n\n# Create DynamoDB resource\ndynamodb = boto3.resource("dynamodb")\ntable = dynamodb.Table("Users")\n\n# Put item\ntable.put_item(Item={\n    "UserId": "user-123",\n    "Email": "user@example.com",\n    "Name": "Jane Doe",\n    "CreatedAt": "2024-01-15T10:30:00Z"\n})\n\n# Query by partition key\nresponse = table.query(\n    KeyConditionExpression=Key("UserId").eq("user-123")\n)\nfor item in response["Items"]:\n    print(item)\n```',
	'\n\n## Best Practices\n\n1. **Design for access patterns first** — model data around queries, not entities\n2. **Use composite sort keys** — e.g., `STATUS#TIMESTAMP` for filtering + sorting\n3. **Avoid hot partitions** — distribute writes across partition keys evenly\n4. **Use sparse indexes** — only items with the index attribute appear in the GSI\n5. **Enable Point-in-Time Recovery** — continuous backups with 35-day retention\n6. **Use TTL for expiring data** — automatic deletion at no extra cost\n7. **Monitor with CloudWatch** — track throttling, consumed capacity, latency\n8. **Use batch operations** — `BatchWriteItem` and `BatchGetItem` for bulk ops\n9. **Consider single-table design** — reduces round trips, simplifies access patterns\n10. **Test with DynamoDB Local** — free local emulator for development',
];

const App = () => {
	const { exit } = useApp();
	const [chunkIndex, setChunkIndex] = useState(1);
	const rows = process.stdout.rows || 24;
	const viewportHeight = Math.max(5, rows - CHROME);

	useInput((input, key) => {
		if (key.ctrl && input === 'c') exit();
		if (input === ' ') setChunkIndex(prev => Math.min(prev + 1, CHUNKS.length));
	});

	const content = CHUNKS.slice(0, chunkIndex).join('');
	const lineCount = content.split('\n').length;
	const exceeds = lineCount > viewportHeight;

	return (
		<Box flexDirection="column">
			<Box borderStyle="round" borderColor="cyan">
				<Text bold color="cyan"> Markdown StreamingPanel </Text>
				<Text> {lineCount} lines | vp={viewportHeight} | {exceeds ? '⚠ SCROLL' : '✓ fits'} | chunk {chunkIndex}/{CHUNKS.length}</Text>
			</Box>

			<StreamingPanel content={content} streaming={true} height={viewportHeight}>
				{(visible) => {
					// Simulate TUI's MarkdownRenderer: each line wrapped in its own Box
					const visLines = visible.split('\n');
					return (
						<StatusBar lineCount={visLines.length}>
							<Box flexDirection="column">
								{visLines.map((l, i) => (
									<Box key={i}>
										<Text wrap="wrap">{l}</Text>
									</Box>
								))}
							</Box>
						</StatusBar>
					);
				}}
			</StreamingPanel>

			<Text dimColor>  Space = add chunk | Ctrl+C = quit</Text>
		</Box>
	);
};

render(<App />);
