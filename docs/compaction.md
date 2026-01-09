# Conversation Compaction

Compaction summarizes older messages while retaining recent ones, freeing up context window space.

- **Manual**: Run `/compact`
- **Automatic**: Triggers when context window overflows

## Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `compaction.excludeMessages` | 2 | Minimum message pairs to retain |
| `compaction.excludeContextWindowPercent` | 2 | Minimum % of context window to retain |

Both are evaluated; the more conservative (larger) value wins.

Compaction creates a new session. Resume the original anytime via `/chat resume`.
