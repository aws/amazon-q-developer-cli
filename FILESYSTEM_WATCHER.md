# Filesystem Watcher Feature

This document describes the filesystem watching functionality added to Amazon Q Developer CLI to address issue #1173.

## Overview

The `fs_watch` tool provides real-time filesystem monitoring capabilities, allowing the AI assistant to be notified when files or directories are modified, created, or deleted. This enables more dynamic interactions and helps keep the conversation in sync with code changes.

## Features

- **Real-time file monitoring**: Watch files and directories for changes
- **Debounced events**: Configurable debouncing to prevent spam from rapid changes
- **Recursive watching**: Option to watch directories recursively or non-recursively  
- **Permission system**: Integrated with existing glob-based permission controls
- **Multiple operations**: Add, remove, list, and stop watching functionality
- **Cross-platform**: Uses the `notify` crate for cross-platform compatibility

## Usage

### AI Tool Interface

The `fs_watch` tool accepts the following operations:

#### Add Watcher
```json
{
  "operation": {
    "type": "add",
    "paths": ["/path/to/watch", "/another/path"],
    "recursive": true,
    "debounce_ms": 500
  }
}
```

#### Remove Watcher
```json
{
  "operation": {
    "type": "remove", 
    "paths": ["/path/to/stop/watching"]
  }
}
```

#### List Watchers
```json
{
  "operation": {
    "type": "list"
  }
}
```

#### Stop All Watchers
```json
{
  "operation": {
    "type": "stop"
  }
}
```

### Parameters

- **type**: Operation type (`add`, `remove`, `list`, `stop`)
- **paths**: Array of file/directory paths to watch/unwatch (required for `add`/`remove`)
- **recursive**: Whether to watch directories recursively (default: `true`, only for `add`)
- **debounce_ms**: Debounce time in milliseconds (default: `500`, range: 50-5000, only for `add`)

## Architecture

### Core Components

1. **FsWatch Tool** (`src/cli/chat/tools/fs_watch.rs`)
   - Main tool implementation
   - Handles operation parsing and validation
   - Manages watcher lifecycle

2. **Tool Integration** (`src/cli/chat/tools/mod.rs`)
   - Integrated into the existing tool system
   - Follows same patterns as other native tools

3. **Tool Manager** (`src/cli/chat/tool_manager.rs`)
   - Added parsing support for `fs_watch` tool
   - Maps tool name to implementation

4. **Tool Schema** (`src/cli/chat/tools/tool_index.json`)
   - JSON schema definition for AI model
   - Defines available operations and parameters

### Technology Stack

- **notify v6.1**: Cross-platform filesystem notification library
- **notify-debouncer-mini v0.4**: Debounced event handling
- **tokio**: Async runtime for background watching
- **serde**: JSON serialization/deserialization

### Event Flow

1. User or AI requests to watch a path
2. `fs_watch` tool validates permissions and paths
3. Creates debounced watcher using `notify` library
4. Background task monitors filesystem events
5. Events are debounced and filtered
6. Notifications can be sent to chat session (future enhancement)

## Implementation Status

### Completed ✅

- [x] Core `fs_watch` tool implementation
- [x] Integration with tool system (enum, traits, imports)
- [x] Tool parsing and registration in tool manager
- [x] JSON schema definition for AI model
- [x] Permission system integration
- [x] Dependency management (Cargo.toml updates)
- [x] Cross-platform support via `notify` crate
- [x] Debounced event handling
- [x] Multiple operation types (add, remove, list, stop)

### Pending 🚧

- [ ] **Session Integration**: Background watcher service and event channels
- [ ] **Event Processing**: Integration with chat session for real-time notifications
- [ ] **Configuration**: Agent-level configuration options
- [ ] **Testing**: Unit and integration tests
- [ ] **Error Handling**: Enhanced error scenarios and recovery
- [ ] **Documentation**: User-facing documentation and examples

## Future Enhancements

1. **Auto-watch**: Automatically watch git repository root
2. **Pattern Filtering**: Include/exclude specific file patterns
3. **Content Hints**: Optional file content snippets with change notifications
4. **IDE Integration**: Better integration with IDE workflows
5. **Performance Optimization**: Handling very large codebases
6. **Event Context**: More detailed event information (file diffs, change types)

## Configuration Example

Future agent configuration might look like:

```toml
[tools.fs_watch]
enabled = true
auto_watch_project = true
watch_patterns = ["src/**/*.rs", "*.toml", "*.md"] 
ignore_patterns = ["target/**", ".git/**", "*.tmp"]
debounce_ms = 500
max_watched_files = 1000
```

## Technical Notes

- The implementation uses `Arc<RwLock<HashMap>>` to manage multiple watchers safely across async contexts
- Debouncing prevents spam from rapid file changes (e.g., during compilation)
- The tool respects the existing permission system used by other file tools
- Error handling gracefully degrades when paths can't be watched
- Memory usage is bounded by the number of watched paths

## Contributing

When extending this feature:

1. Follow existing tool patterns in the codebase
2. Add appropriate tests for new functionality
3. Update the JSON schema for any new parameters
4. Consider cross-platform compatibility
5. Respect the existing permission and security model

## Related Issues

- GitHub Issue #1173: Feature request for filesystem watching
- Related to Aider's filesystem watching capabilities
- Supports IDE integration workflows