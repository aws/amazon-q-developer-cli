---
doc_meta:
  validated: 2025-12-22
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: feature
  title: Knowledge Management
  description: Persistent knowledge base with semantic search, agent isolation, and auto-sync capabilities
  keywords: [knowledge, base, semantic, search, persistent, agent, isolation]
  related: [knowledge-tool, slash-knowledge, enable-knowledge]
---

# Knowledge Management

Persistent knowledge base with semantic search, agent isolation, and auto-sync capabilities.

## Overview

Knowledge Management provides persistent storage and semantic search across chat sessions. Each agent maintains isolated knowledge bases. Supports two index types (Fast/Best), pattern filtering, background indexing, and auto-sync with agent resources.

## Enabling

```bash
kiro-cli settings chat.enableKnowledge true
```

Experimental feature, disabled by default.

## Index Types

### Fast (Lexical - BM25)
- ✅ Lightning-fast indexing
- ✅ Instant keyword search
- ✅ Low resource usage
- ✅ Perfect for logs, configs, large codebases
- ❌ Requires exact keyword matches

### Best (Semantic - all-MiniLM-L6-v2)
- ✅ Understands context and meaning
- ✅ Natural language queries
- ✅ Finds related concepts
- ✅ Perfect for documentation, research
- ❌ Slower indexing
- ❌ Higher resource usage

**When to use**:
| Use Case | Type | Why |
|----------|------|-----|
| Logs, errors | Fast | Quick keyword searches |
| Config files | Fast | Exact parameter lookups |
| Large codebases | Fast | Fast symbol searches |
| Documentation | Best | Natural language understanding |
| Research | Best | Concept-based searching |

**Set default**:
```bash
kiro-cli settings knowledge.indexType Fast
```

## Agent Isolation

Each agent has its own isolated knowledge base.

**Storage structure**:
```
~/.kiro/knowledge_bases/
├── q_cli_default/              # Default agent
│   ├── contexts.json
│   └── context-id-1/
│       ├── data.json
│       └── bm25_data.json
├── my-agent_abc123/            # Custom agent
│   ├── contexts.json
│   └── context-id-2/
│       └── data.json
```

**Behavior**:
- Agent A cannot access Agent B's knowledge
- Switching agents switches knowledge bases
- Independent configuration per agent

## Auto-Sync with Agent Resources

Define knowledge bases in agent configuration for automatic management:

```json
{
  "resources": [
    {
      "type": "knowledgeBase",
      "source": "file://./docs",
      "name": "Documentation",
      "indexType": "best",
      "include": ["**/*.md"],
      "exclude": ["**/draft/**"],
      "autoUpdate": true
    }
  ]
}
```

**Auto-sync behavior**:
- ✅ Automatically added when agent loads
- ✅ Automatically removed when deleted from config
- ✅ Automatically updated when config changes
- ✅ Re-indexed on agent load if `autoUpdate: true`

**vs Manual (`/knowledge add`)**:
- Manual entries persist independently
- Not affected by agent config changes
- Require manual updates

## Pattern Filtering

Control which files are indexed:

```bash
/knowledge add -n "rust-code" -p ./src --include "**/*.rs" --exclude "target/**"
```

**Pattern syntax**:
- `*.rs` - All .rs files recursively
- `**/*.py` - All Python files
- `target/**` - Everything in target/
- `node_modules/**` - Everything in node_modules/

**Default patterns**:
```bash
kiro-cli settings knowledge.defaultIncludePatterns '["**/*.rs", "**/*.md"]'
kiro-cli settings knowledge.defaultExcludePatterns '["target/**", "node_modules/**"]'
```

## Supported File Types

**Text**: .txt, .log, .rtf, .tex, .rst  
**Markdown**: .md, .markdown, .mdx  
**JSON**: .json  
**Config**: .ini, .conf, .cfg, .properties, .env  
**Data**: .csv, .tsv  
**Web**: .svg  
**Code**: .rs, .py, .js, .jsx, .ts, .tsx, .java, .c, .cpp, .h, .hpp, .go, .rb, .php, .swift, .kt, .cs, .sh, .bash, .zsh, .html, .xml, .css, .scss, .sql, .yaml, .yml, .toml  
**Special**: Dockerfile, Makefile, LICENSE, CHANGELOG, README

Unsupported files indexed without text extraction.

## Configuration

```bash
kiro-cli settings knowledge.maxFiles 10000
kiro-cli settings knowledge.chunkSize 1024
kiro-cli settings knowledge.chunkOverlap 256
kiro-cli settings knowledge.indexType Fast
kiro-cli settings knowledge.defaultIncludePatterns '["**/*.rs"]'
kiro-cli settings knowledge.defaultExcludePatterns '["target/**"]'
```

## Best Practices

### Organizing
- Use descriptive names
- Group related files in directories
- Use patterns to focus on relevant files
- Review and update outdated contexts

### Searching
- Use natural language queries
- Be specific about what you're looking for
- Try different phrasings
- Prompt: "find X using your knowledge bases"

### Large Projects
- Add directories, not individual files
- Exclude build artifacts
- Monitor indexing progress with `/knowledge show`
- Break very large projects into logical parts

## Troubleshooting

### Files Not Indexed

**Causes**:
- Include patterns don't match files
- Exclude patterns filtering out files
- Unsupported file types
- Indexing still in progress

**Solutions**:
- Check patterns with `/knowledge show`
- Verify file extensions supported
- Wait for indexing to complete
- Check paths exist and accessible

### Search Not Finding Results

**Causes**:
- Indexing incomplete
- Query doesn't match content
- Content not added

**Solutions**:
- Check `/knowledge show` for completion
- Try different query phrasings
- Verify content was added

### Performance Issues

**Causes**:
- Large directories
- Too many files
- Complex patterns

**Solutions**:
- Use `/knowledge cancel` to stop operations
- Add smaller chunks
- Use better exclude patterns
- Adjust maxFiles setting

## Related

- [knowledge tool](../tools/knowledge.md) - Tool parameters
- [/knowledge](../slash-commands/knowledge.md) - Slash commands
- [chat.enableKnowledge](../settings/enable-knowledge.md) - Enable setting

## Limitations

- Beta feature (may change)
- Binary files ignored
- Large files chunked (may split content)
- No storage size limits (practical limits apply)
- No automatic cleanup
- Clear operations irreversible
