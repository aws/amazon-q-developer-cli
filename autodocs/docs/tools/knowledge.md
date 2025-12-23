---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: true
  category: tool
  title: knowledge
  description: Store and retrieve information across chat sessions with semantic search capabilities
  keywords: [knowledge, store, search, semantic, persistent, memory]
  related: [slash-knowledge, enable-knowledge]
---

# knowledge

Store and retrieve information across chat sessions with semantic search capabilities.

## Overview

The knowledge tool provides persistent information storage across chat sessions. Store files, directories, or text snippets, then search semantically. Experimental feature requiring explicit enablement.

## Usage

### Basic Usage

```json
{
  "command": "add",
  "name": "project-docs",
  "value": "docs/**/*.md"
}
```

### Common Use Cases

#### Use Case 1: Add Files to Knowledge Base

```json
{
  "command": "add",
  "name": "api-docs",
  "value": "src/api/**/*.ts"
}
```

**What this does**: Indexes all TypeScript files in src/api/ for semantic search.

#### Use Case 2: Add Text Note

```json
{
  "command": "add",
  "name": "deployment-notes",
  "value": "Deployment requires:\n1. Run tests\n2. Build production\n3. Deploy to staging first"
}
```

**What this does**: Stores text note in knowledge base.

#### Use Case 3: Search Knowledge Base

```json
{
  "command": "search",
  "query": "authentication flow",
  "limit": 5
}
```

**What this does**: Semantic search for authentication-related content.

#### Use Case 4: Remove Entry

```json
{
  "command": "remove",
  "name": "old-docs"
}
```

**What this does**: Removes knowledge base entry by name.

#### Use Case 5: Update Entry

```json
{
  "command": "update",
  "name": "api-docs",
  "path": "src/api/**/*.ts"
}
```

**What this does**: Re-indexes files for existing entry.

## Configuration

Enable knowledge feature:

```bash
kiro-cli settings chat.enableKnowledge true
```

No agent configuration needed - knowledge is trusted by default.

## Commands

### add

Add files or text to knowledge base.

**Parameters**:
- `name` (string, required): Entry identifier
- `value` (string, required): File path/glob or text content

### remove

Remove entry from knowledge base.

**Parameters**:
- `name` (string): Entry name
- `context_id` (string): Entry ID
- `path` (string): File path

At least one parameter required.

### search

Search knowledge base semantically.

**Parameters**:
- `query` (string, required): Search query
- `context_id` (string, optional): Limit to specific entry
- `limit` (integer, optional): Max results
- `offset` (integer, optional): Pagination offset
- `snippet_length` (integer, optional): Result snippet length
- `sort_by` (string, optional): Sort order
- `file_type` (string, optional): Filter by file type

### update

Re-index existing entry.

**Parameters**:
- `name` (string): Entry name
- `context_id` (string): Entry ID
- `path` (string): File path

At least one parameter required.

### show

List all knowledge base entries.

**Parameters**: None

### clear

Clear entire knowledge base.

**Parameters**:
- `confirm` (boolean, required): Must be true

### cancel

Cancel background operation.

**Parameters**:
- `operation_id` (string, required): Operation ID or "all"

## Examples

### Example 1: Index Documentation

```json
{
  "command": "add",
  "name": "rust-docs",
  "value": "docs"
}
```

Indexes all files in docs/ directory.

### Example 2: Store Meeting Notes

```json
{
  "command": "add",
  "name": "meeting-2024-12-19",
  "value": "Discussed: API redesign, performance improvements, Q1 roadmap"
}
```

Stores text note.

### Example 3: Search for Information

```json
{
  "command": "search",
  "query": "error handling patterns",
  "limit": 3
}
```

### Example 4: List All Entries

```json
{
  "command": "show"
}
```

## Troubleshooting

### Issue: "Knowledge feature not enabled"

**Symptom**: Tool returns error  
**Cause**: Feature not enabled  
**Solution**: `kiro-cli settings chat.enableKnowledge true`

### Issue: Path Not Found

**Symptom**: "Path does not exist" error  
**Cause**: Invalid file path  
**Solution**: Verify path exists. Use relative or absolute paths.

### Issue: No Search Results

**Symptom**: Empty search results  
**Cause**: Query doesn't match indexed content  
**Solution**: Try different search terms or verify content was indexed.

### Issue: Slow Indexing

**Symptom**: Add operation takes long time  
**Cause**: Large number of files or large files  
**Solution**: Use more specific glob patterns or index in smaller batches.

## Related Features

- [/knowledge](../slash-commands/knowledge.md) - Slash commands for knowledge management
- [chat.enableKnowledge](../settings/enable-knowledge.md) - Enable setting
- [fs_read](fs-read.md) - Read files for indexing

## Limitations

- Experimental feature (may change)
- Requires explicit enablement
- Storage location not configurable
- No cloud sync
- Semantic search quality depends on content
- Large files may be slow to index

## Technical Details

**Aliases**: `knowledge`

**Storage**: Local knowledge base in workspace

**Indexing**: Background indexing for large operations

**Search**: Semantic search using embeddings

**Permissions**: Trusted by default. Requires `chat.enableKnowledge` setting enabled.
