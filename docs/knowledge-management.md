# Knowledge Management

The /knowledge command provides persistent knowledge base functionality for Amazon Q CLI, allowing you to store, search, and manage contextual information that persists across chat sessions.

> [!NOTE]
> This is a beta feature that must be enabled before use.

## Getting Started

Enable the Knowledge Feature

The knowledge feature is experimental and disabled by default. Enable it with:

`q settings chat.enableKnowledge true`

## Basic Usage

Once enabled, you can use `/knowledge` commands within your chat session:

`/knowledge add myproject /path/to/project`
`/knowledge show`

## Commands

#### `/knowledge show`

Display all entries in your knowledge base with detailed information including creation dates, item counts, and persistence status.

#### `/knowledge add <name> <path> [--include pattern] [--exclude pattern] [--index-type Fast|Best]`

Add files or directories to your knowledge base. The system will recursively index all supported files in directories.

`/knowledge add "project-docs" /path/to/documentation`
`/knowledge add "config-files" /path/to/config.json`
`/knowledge add "fast-search" /path/to/logs --index-type Fast`
`/knowledge add "semantic-search" /path/to/docs --index-type Best`

**Index Types**

Choose the indexing approach that best fits your needs:

- **`--index-type Fast`** (Lexical - BM25): 
  - ✅ **Lightning-fast indexing** - processes files quickly
  - ✅ **Instant search** - keyword-based search with immediate results
  - ✅ **Low resource usage** - minimal CPU and memory requirements
  - ✅ **Perfect for logs, configs, and large codebases**
  - ❌ Less intelligent - requires exact keyword matches

- **`--index-type Best`** (Semantic - all-MiniLM-L6-v2):
  - ✅ **Intelligent search** - understands context and meaning
  - ✅ **Natural language queries** - search with full sentences
  - ✅ **Finds related concepts** - even without exact keyword matches
  - ✅ **Perfect for documentation, research, and complex content**
  - ❌ Slower indexing - requires AI model processing
  - ❌ Higher resource usage - more CPU and memory intensive

**When to Use Each Type:**

| Use Case | Recommended Type | Why |
|----------|------------------|-----|
| Log files, error messages | `Fast` | Quick keyword searches, large volumes |
| Configuration files | `Fast` | Exact parameter/value lookups |
| Large codebases | `Fast` | Fast symbol and function searches |
| Documentation | `Best` | Natural language understanding |
| Research papers | `Best` | Concept-based searching |
| Mixed content | `Best` | Better overall search experience |

**Default Behavior:**

If you don't specify `--index-type`, the system uses your configured default:

```bash
# Set your preferred default
q settings knowledge.indexType Fast   # or Best

# This will use your default setting
/knowledge add "my-project" /path/to/project
```

**Default Pattern Behavior**

When you don't specify `--include` or `--exclude` patterns, the system uses your configured default patterns:

- If no patterns are specified and no defaults are configured, all supported files are indexed
- Default include patterns apply when no `--include` is specified
- Default exclude patterns apply when no `--exclude` is specified
- Explicit patterns always override defaults

Example with defaults configured:
```bash
q settings knowledge.defaultIncludePatterns '["**/*.rs", "**/*.py"]'
q settings knowledge.defaultExcludePatterns '["target/**", "__pycache__/**"]'

# This will use the default patterns
/knowledge add "my-project" /path/to/project

# This will override defaults with explicit patterns
/knowledge add "docs-only" /path/to/project --include "**/*.md"
```

**New: Pattern Filtering**

You can now control which files are indexed using include and exclude patterns:

`/knowledge add "rust-code" /path/to/project --include "*.rs" --exclude "target/**"`
`/knowledge add "docs" /path/to/project --include "**/*.md" --include "**/*.txt" --exclude "node_modules/**"`

Pattern examples:
- `*.rs` - All Rust files in all directories recursively (equivalent to `**/*.rs`)
- `**/*.py` - All Python files recursively
- `target/**` - Everything in target directory
- `node_modules/**` - Everything in node_modules directory

Supported file types (expanded):

- Text files: .txt, .log, .rtf, .tex, .rst
- Markdown: .md, .markdown, .mdx (now supported!)
- JSON: .json (now treated as text for better searchability)
- Configuration: .ini, .conf, .cfg, .properties, .env
- Data files: .csv, .tsv
- Web formats: .svg (text-based)
- Code files: .rs, .py, .js, .jsx, .ts, .tsx, .java, .c, .cpp, .h, .hpp, .go, .rb, .php, .swift, .kt, .kts, .cs, .sh, .bash, .zsh, .html, .htm, .xml, .css, .scss, .sass, .less, .sql, .yaml, .yml, .toml
- Special files: Dockerfile, Makefile, LICENSE, CHANGELOG, README (files without extensions)

> Important: Unsupported files are indexed without text content extraction.

#### `/knowledge remove <identifier>`

Remove entries from your knowledge base. You can remove by name, path, or context ID.

`/knowledge remove "project-docs"` # Remove by name
`/knowledge remove /path/to/old/project` # Remove by path

#### `/knowledge update <path>`

Update an existing knowledge base entry with new content from the specified path. The original include/exclude patterns are preserved during updates.

`/knowledge update /path/to/updated/project`

#### `/knowledge clear`

Remove all entries from your knowledge base. This action requires confirmation and cannot be undone.

You'll be prompted to confirm:

> ⚠️ This will remove ALL knowledge base entries. Are you sure? (y/N):

#### `/knowledge status`

View the status of background indexing operations, including progress and queue information.

#### `/knowledge cancel [operation_id]`

Cancel background operations. You can cancel a specific operation by ID or all operations if no ID is provided.

`/knowledge cancel abc12345 # Cancel specific operation`
`/knowledge cancel all # Cancel all operations`

## Configuration

Configure knowledge base behavior:

`q settings knowledge.maxFiles 10000` # Maximum files per knowledge base
`q settings knowledge.chunkSize 1024` # Text chunk size for processing
`q settings knowledge.chunkOverlap 256` # Overlap between chunks
`q settings knowledge.indexType Fast` # Default index type (Fast or Best)
`q settings knowledge.defaultIncludePatterns '["**/*.rs", "**/*.md"]'` # Default include patterns
`q settings knowledge.defaultExcludePatterns '["target/**", "node_modules/**"]'` # Default exclude patterns

## Agent-Specific Knowledge Bases

### Isolated Knowledge Storage

Each agent maintains its own isolated knowledge base, ensuring that knowledge contexts are scoped to the specific agent you're working with. This provides better organization and prevents knowledge conflicts between different agents.

### Folder Structure

Knowledge bases are stored in the following directory structure:

```
~/.aws/amazonq/knowledge_bases/
├── q_cli_default/          # Default agent knowledge base
│   ├── contexts.json       # Metadata for all contexts
│   ├── context-id-1/       # Individual context storage
│   │   ├── data.json       # Semantic search data
│   │   └── bm25_data.json  # BM25 search data (if using Fast index)
│   └── context-id-2/
│       ├── data.json
│       └── bm25_data.json
├── my-custom-agent_<alphanumeric-code>/        # Custom agent knowledge base
│   ├── contexts.json
│   ├── context-id-3/
│   │   └── data.json
│   └── context-id-4/
│       └── data.json
└── another-agent_<alphanumeric-code>/          # Another agent's knowledge base
    ├── contexts.json
    └── context-id-5/
        └── data.json
```

### How Agent Isolation Works

- **Automatic Scoping**: When you use `/knowledge` commands, they automatically operate on the current agent's knowledge base
- **No Cross-Agent Access**: Agent A cannot access or search knowledge contexts created by Agent B
- **Independent Configuration**: Each agent can have different knowledge base settings and contexts
- **Migration Support**: Legacy knowledge bases are automatically migrated to the default agent on first use

### Agent Switching

When you switch between agents, your knowledge commands will automatically work with that agent's specific knowledge base:

```bash
# Working with default agent
/knowledge add /path/to/docs

# Switch to custom agent
q chat --agent my-custom-agent

# This creates a separate knowledge base for my-custom-agent
/knowledge add /path/to/agent/docs

# Switch back to default
q chat

# Only sees the original project-docs, not agent-specific-docs
/knowledge show
```

## How It Works

#### Indexing Process

When you add content to the knowledge base:

1. **Pattern Filtering**: Files are filtered based on include/exclude patterns (if specified)
2. **File Discovery**: The system recursively scans directories for supported file types
3. **Content Extraction**: Text content is extracted from each supported file
4. **Chunking**: Large files are split into smaller, searchable chunks
5. **Background Processing**: Indexing happens asynchronously in the background
6. **Semantic Embedding**: Content is processed for semantic search capabilities

#### Search Capabilities

The knowledge base uses semantic search, which means:

- You can search using natural language queries
- Results are ranked by relevance, not just keyword matching
- Related concepts are found even if exact words don't match

#### Persistence

- Persistent contexts: Survive across chat sessions and CLI restarts
- Context persistence is determined automatically based on usage patterns
- Include/exclude patterns are stored with each context and reused during updates

#### Best Practices

Organizing Your Knowledge Base

- Use descriptive names when adding contexts: "api-documentation" instead of "docs"
- Group related files in directories before adding them
- Use include/exclude patterns to focus on relevant files
- Regularly review and update outdated contexts

#### Effective Searching

- Use natural language queries: "how to handle authentication errors using the knowledge tool"
- Be specific about what you're looking for: "database connection configuration"
- Try different phrasings if initial searches don't return expected results
- Prompt Q to use the tool with prompts like "find database connection configuration using your knowledge bases" or "using your knowledge tools can you find how to replace your laptop"

#### Managing Large Projects

- Add project directories rather than individual files when possible
- Use include/exclude patterns to avoid indexing build artifacts: `--exclude "target/**" --exclude "node_modules/**"`
- Use /knowledge status to monitor indexing progress for large directories
- Consider breaking very large projects into logical sub-directories

#### Pattern Filtering Best Practices

- **Be specific**: Use precise patterns to avoid over-inclusion
- **Exclude build artifacts**: Always exclude directories like `target/**`, `node_modules/**`, `.git/**`
- **Include relevant extensions**: Focus on file types you actually need to search
- **Test patterns**: Verify patterns match expected files before large indexing operations

## Limitations

#### File Type Support

- Binary files are ignored during indexing
- Very large files may be chunked, potentially splitting related content
- Some specialized file formats may not extract content optimally

#### Performance Considerations

- Large directories may take significant time to index
- Background operations are limited by concurrent processing limits
- Search performance may vary based on knowledge base size and embedding engine
- Pattern filtering happens during file walking, improving performance for large directories

#### Storage and Persistence

- No explicit storage size limits, but practical limits apply
- No automatic cleanup of old or unused contexts
- Clear operations are irreversible with no backup functionality

## Troubleshooting

#### Files Not Being Indexed

If your files aren't appearing in search results:

1. **Check patterns**: Ensure your include patterns match the files you want
2. **Verify exclude patterns**: Make sure exclude patterns aren't filtering out desired files
3. **Check file types**: Ensure your files have supported extensions
4. **Monitor status**: Use /knowledge status to check if indexing is still in progress
5. **Verify paths**: Ensure the paths you added actually exist and are accessible
6. **Check for errors**: Look for error messages in the CLI output

#### Search Not Finding Expected Results

If searches aren't returning expected results:

1. **Wait for indexing**: Use /knowledge status to ensure indexing is complete
2. **Try different queries**: Use various phrasings and keywords
3. **Verify content**: Use /knowledge show to confirm your content was added
4. **Check file types**: Unsupported file types won't have searchable content

#### Performance Issues

If operations are slow:

1. **Check queue status**: Use /knowledge status to see operation queue
2. **Cancel if needed**: Use /knowledge cancel to stop problematic operations
3. **Add smaller chunks**: Consider adding subdirectories instead of entire large projects
4. **Use better patterns**: Exclude unnecessary files with exclude patterns
5. **Adjust settings**: Consider lowering maxFiles or chunkSize for better performance

#### Pattern Issues

If patterns aren't working as expected:

1. **Test patterns**: Use simple patterns first, then add complexity
2. **Check syntax**: Ensure glob patterns use correct syntax (`**` for recursive, `*` for single level)
3. **Verify paths**: Make sure patterns match actual file paths in your project
4. **Use absolute patterns**: Consider using full paths in patterns for precision
