# 📚 Q CLI History Feature

A comprehensive conversation management system for Amazon Q Developer CLI that allows you to browse, search, export, and restore your chat history.

## 🎯 Overview

The Q CLI automatically saves your conversations to a local SQLite database. This history feature provides powerful tools to:

- **Browse** your conversation history with filtering
- **Search** through conversation content 
- **Export** conversations in multiple formats (JSON, Markdown, Text)
- **Restore** conversations to continue them later
- **Seamlessly integrate** with existing `/save` and `/load` commands

## 🚀 Features

### 📋 List & Filter Conversations
```bash
q history list                              # Show recent conversations
q history list --limit 20                  # Show more conversations
q history list --contains "aws"            # Filter by content
q history list --path "/workspace"         # Filter by directory path
```

### 🔍 Search Conversation Content
```bash
q history search "gitignore"               # Search for specific topics
q history search "ec2 instances" --limit 5 # Limit search results
```

### 👀 View Conversation Details
```bash
q history show 42c8750d                    # Show full conversation
```

### 📤 Export Conversations
```bash
# Export as JSON (compatible with /load command)
q history export 42c8750d --output conversation.json

# Export as Markdown for documentation
q history export 42c8750d --output conversation.md --format markdown

# Export as plain text for reading
q history export 42c8750d --output conversation.txt --format text

# Force overwrite existing files
q history export 42c8750d --output existing.json --force
```

### 🔄 Restore & Resume Conversations
```bash
q history restore 42c8750d                 # Copy conversation to current directory
q chat --resume                            # Resume the conversation
```

### 🔗 Integration with /save and /load
```bash
# Export from history and import in any chat session
q history export 42c8750d --output shared.json
# In any chat session: /load shared.json
```

## 📊 Sample Output

### List Command
```
┌──────────┬─────────────────────┬──────────────────────────────────────────────────┬─────────────────────────────────────┐
│ ID       │ Date                │ Directory                                        │ Preview                             │
├──────────┼─────────────────────┼──────────────────────────────────────────────────┼─────────────────────────────────────┤
│ 87442abe │ 2025-08-03 20:45:32 │ .../amazon-q-developer-cli                      │ what's the best ec2 feature?       │
│ 42c8750d │ 2025-08-03 19:30:15 │ .../userguide                                   │ help me create a gitignore file     │
└──────────┴─────────────────────┴──────────────────────────────────────────────────┴─────────────────────────────────────┘

To show a conversation: q history show <ID>
To search conversations: q history search <query>
To export a conversation: q history export <ID> --output <file>
To restore a conversation to current directory: q history restore <ID>
To resume a conversation, navigate to the directory and run `q chat --resume`
```

### Export Success
```
✅ Exported conversation 87442abe as JSON (compatible with /load) to 'conversation.json'

Conversation: 87442abe-de53-4b0d-888c-e7a9dadf2a92
Original directory: /workspace/amazon-q-developer-cli
Messages: 3

💡 You can import this conversation in any chat session with:
   /load conversation.json
```

## 🛠 How to Try It Yourself

### Prerequisites
- Git
- Rust toolchain (rustc, cargo)
- Amazon Q Developer CLI account

### Step 1: Clone the Fork
```bash
git clone https://github.com/mibeco/amazon-q-developer-cli.git
cd amazon-q-developer-cli
git checkout feature/chat-history-browsing
```

### Step 2: Build the Project
```bash
cargo build --release
```

The binary will be created at `./target/release/chat_cli`

### Step 3: Handle Existing Q CLI Installation

⚠️ **Important**: Check if you already have a `q` command installed:
```bash
which q
```

If this returns a path (like `/Users/username/.local/bin/q`), you have a conflicting installation. Choose one of these approaches:

**Option A: Create a symlink (recommended)**
```bash
# Create a symlink with a different name to avoid conflicts
sudo ln -s /path/to/amazon-q-developer-cli/target/release/chat_cli /usr/local/bin/qdev

# Test it works
qdev history --help
```

**Option B: Use full path (for testing without permanent changes)**
```bash
# Test the history feature directly
./target/release/chat_cli history --help
```

**Option C: Create a wrapper script**
```bash
# Create a test script in your project directory
echo '#!/bin/bash' > qtest
echo "$(pwd)/target/release/chat_cli \"\$@\"" >> qtest
chmod +x qtest

# Test it
./qtest history --help
```

**Option D: Use shell alias**
```bash
# Add to your shell profile (~/.bashrc, ~/.zshrc, etc.)
alias qdev='/path/to/amazon-q-developer-cli/target/release/chat_cli'

# Reload your shell profile
source ~/.zshrc  # or source ~/.bashrc
```

### Step 4: Verify Installation
```bash
# If you used the symlink approach (recommended):
qdev --version                    # Should show version 1.13.1 or higher
qdev history --help               # Should show history subcommands

# If you used a different approach, replace 'qdev' with your chosen method
```

If the history command isn't available, see the Troubleshooting section below.

### Step 5: Generate Some History
```bash
# Have a few conversations to create history
qdev chat
# Ask some questions, then exit with /quit

# Repeat a few times in different directories to build up history
```

### Step 6: Try the History Features
```bash
# List your conversations
qdev history list

# Search for specific content
qdev history search "your search term"

# Show a specific conversation (use an ID from the list)
qdev history show <ID>

# Export a conversation
qdev history export <ID> --output my_conversation.json

# Try different export formats
qdev history export <ID> --output conversation.md --format markdown
qdev history export <ID> --output conversation.txt --format text

# Restore a conversation to current directory
qdev history restore <ID>
```

## 🐛 Troubleshooting

### History Command Not Found
If you get "unrecognized subcommand 'history'":

1. **Verify you're using the right binary:**
   ```bash
   which your_command
   # Should point to your built binary, not an existing installation
   ```

2. **If using an alias, check it's set correctly:**
   ```bash
   alias | grep your_alias_name
   ```

3. **Try using the full path directly:**
   ```bash
   /full/path/to/amazon-q-developer-cli/target/release/chat_cli history list
   ```

### No Conversations Found
- Ensure you've had some chat sessions with the built Q CLI
- Check that conversations completed successfully (not interrupted)
- Verify database location: `~/.aws/amazonq/`

### Export Fails
- Check file permissions in target directory
- Use `--force` flag to overwrite existing files
- Verify conversation ID exists with `your_command history list`

### Import Issues
- Ensure JSON file was exported from Q CLI (not manually created)
- Check file integrity and formatting
- Use `/load` command within a chat session, not from command line

## 🎨 Export Formats

### JSON Format
- **Purpose**: Full fidelity backup and sharing
- **Compatibility**: Can be imported with `/load` command in any chat session
- **Content**: Complete conversation state including tools, context, agents
- **Use case**: Backup, sharing, moving conversations between environments

### Markdown Format
- **Purpose**: Human-readable documentation
- **Features**: Proper headers, code blocks, timestamps
- **Use case**: Documentation, sharing with team members, creating guides

### Text Format
- **Purpose**: Simple reading and sharing
- **Features**: Clean plain text with clear message separation
- **Use case**: Quick reading, email sharing, simple archival

## 🔧 Technical Details

### Database Storage
- **Location**: `~/.aws/amazonq/` (SQLite database)
- **Key**: Directory path where conversation occurred
- **Content**: Full `ConversationState` as JSON
- **Automatic**: Saved after each assistant response

### File Compatibility
- **JSON exports** use identical serialization as `/save` command
- **Perfect compatibility** with existing `/load` functionality
- **Future-proof** design leverages existing infrastructure

### Search Capabilities
- **Full-text search** across all conversation content
- **Contextual previews** showing relevant snippets
- **Flexible filtering** by path, content, and date ranges

## 🤝 Integration with Existing Features

The history feature seamlessly integrates with Q CLI's existing functionality:

1. **Automatic Storage**: Every conversation is automatically saved
2. **Resume Capability**: Use `q chat --resume` in any directory
3. **File Compatibility**: Export/import with `/save` and `/load`
4. **Tool Preservation**: Exported conversations retain all tool configurations
5. **Context Preservation**: Full conversation context is maintained

## 📈 Workflow Examples

### Developer Documentation Workflow
```bash
# 1. Have a conversation about a complex topic
q chat
# Ask: "How do I set up AWS Lambda with API Gateway?"

# 2. Export as documentation
q history export <ID> --output lambda-api-setup.md --format markdown

# 3. Share with team or add to documentation repo
```

### Troubleshooting Archive Workflow
```bash
# 1. Search for previous solutions
q history search "error 403"

# 2. Export relevant conversations
q history export <ID> --output troubleshooting-403.json

# 3. Import in new session when issue recurs
# In chat: /load troubleshooting-403.json
```

### Cross-Environment Workflow
```bash
# 1. Export conversation from development environment
q history export <ID> --output project-setup.json

# 2. Transfer file to production environment
# 3. Import and continue conversation
# In chat: /load project-setup.json
```

## 🐛 Troubleshooting

### No Conversations Found
- Ensure you've had some chat sessions with Q CLI
- Check that conversations completed successfully (not interrupted)
- Verify database location: `~/.aws/amazonq/`

### Export Fails
- Check file permissions in target directory
- Use `--force` flag to overwrite existing files
- Verify conversation ID exists with `q history list`

### Import Issues
- Ensure JSON file was exported from Q CLI (not manually created)
- Check file integrity and formatting
- Use `/load` command within a chat session, not from command line

## 🎉 What's New

This feature adds comprehensive conversation management to Q CLI:

- ✅ **Complete history browsing** with intuitive table layout
- ✅ **Powerful search functionality** with contextual previews  
- ✅ **Multi-format export** (JSON, Markdown, Text)
- ✅ **Seamless integration** with existing `/save`/`/load` commands
- ✅ **Safe conversation restoration** with automatic backups
- ✅ **Robust error handling** with helpful user guidance

## 📝 Command Reference

```bash
q history list [--limit N] [--path PATH] [--contains TEXT]
q history search <query> [--limit N]
q history show <id>
q history export <id> --output <file> [--format FORMAT] [--force]
q history restore <id>
```

**Export Formats**: `json` (default), `markdown`, `text`

---

**Built with ❤️ for the Amazon Q Developer CLI community**

*This feature bridges the gap between automatic conversation storage and manual file-based sharing, giving developers powerful tools to manage their AI-assisted development workflow.*
