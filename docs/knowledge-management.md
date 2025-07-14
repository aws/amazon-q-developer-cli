# Knowledge Management

The /knowledge command provides persistent knowledge base functionality for Amazon Q CLI, allowing you to store, search, and manage contextual information that persists across chat sessions.

> Note: This is a beta feature that must be enabled before use.

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

#### `/knowledge add <name> <path>`

Add files or directories to your knowledge base. The system will recursively index all supported files in directories.

`/knowledge add "project-docs" /path/to/documentation`
`/knowledge add "config-files" /path/to/config.json`

Supported file types:

- Text files: .txt
- Markdown: .md, .markdown
- JSON: .json
- Code files: .rs, .py, .js, .jsx, .ts, .tsx, .java, .c, .cpp, .h, .hpp, .go, .rb, .php, .swift, .kt, .kts, .cs, .sh, .bash, .zsh, .html, .htm, .xml, .css, .scss, .sass, .less, .sql, .yaml, .yml, .toml

> Important: Unsupported files are indexed without text content extraction.

#### `/knowledge remove <identifier>`

Remove entries from your knowledge base. You can remove by name, path, or context ID.

`/knowledge remove "project-docs"` # Remove by name
`/knowledge remove /path/to/old/project` # Remove by path

#### `/knowledge update <path>`

Update an existing knowledge base entry with new content from the specified path.

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

## How It Works

#### Indexing Process

When you add content to the knowledge base:

1. File Discovery: The system recursively scans directories for supported file types
2. Content Extraction: Text content is extracted from each supported file
3. Chunking: Large files are split into smaller, searchable chunks
4. Background Processing: Indexing happens asynchronously in the background
5. Semantic Embedding: Content is processed for semantic search capabilities

#### Search Capabilities

The knowledge base uses semantic search, which means:

- You can search using natural language queries
- Results are ranked by relevance, not just keyword matching
- Related concepts are found even if exact words don't match

#### Persistence

- Persistent contexts: Survive across chat sessions and CLI restarts
- Context persistence is determined automatically based on usage patterns

#### Best Practices

Organizing Your Knowledge Base

- Use descriptive names when adding contexts: "api-documentation" instead of "docs"
- Group related files in directories before adding them
- Regularly review and update outdated contexts

#### Effective Searching

- Use natural language queries: "how to handle authentication errors using the knowledge tool"
- Be specific about what you're looking for: "database connection configuration"
- Try different phrasings if initial searches don't return expected results
- Prompt Q to use the tool with prompts like "find database connection configuration using your knowledge bases" or "using your knowledge tools can you find how to replace your laptop"

#### Managing Large Projects

- Add project directories rather than individual files when possible
- Use /knowledge status to monitor indexing progress for large directories
- Consider breaking very large projects into logical sub-directories

## Limitations

#### File Type Support

- .mdx files are not currently supported for content extraction
- Binary files are ignored during indexing
- Very large files may be chunked, potentially splitting related content.

#### Performance Considerations

- Large directories may take significant time to index
- Background operations are limited by concurrent processing limits
- Search performance may vary based on knowledge base size
- Currently there’s a hard limit of 5k files per knowledge base (getting removed soon as on Jul 12th, 2025).

#### Storage and Persistence

- No explicit storage size limits, but practical limits apply
- No automatic cleanup of old or unused contexts
- Clear operations are irreversible with no backup functionality

## Troubleshooting

#### Files Not Being Indexed

If your files aren't appearing in search results:

1. Check file types: Ensure your files have supported extensions
2. Monitor status: Use /knowledge status to check if indexing is still in progress
3. Verify paths: Ensure the paths you added actually exist and are accessible
4. Check for errors: Look for error messages in the CLI output

#### Search Not Finding Expected Results

If searches aren't returning expected results:

1. Wait for indexing: Use /knowledge status to ensure indexing is complete
2. Try different queries: Use various phrasings and keywords
3. Verify content: Use /knowledge show to confirm your content was added
4. Check file types: Unsupported file types won't have searchable content

#### Performance Issues

If operations are slow:

1. Check queue status: Use /knowledge status to see operation queue
2. Cancel if needed: Use /knowledge cancel to stop problematic operations
3. Add smaller chunks: Consider adding subdirectories instead of entire large projects
