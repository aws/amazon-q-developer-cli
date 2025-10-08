# Q CLI Logging Patterns Analysis

## Overview

The Q CLI codebase uses a sophisticated logging system based on the `tracing` ecosystem rather than simple `println!` statements or basic debug logging. The logging infrastructure is centralized and configurable, supporting multiple output destinations and log levels.

## Logging Framework

### Core Dependencies
From `crates/chat-cli/Cargo.toml`:
- `tracing` - Core structured logging framework
- `tracing-appender` - Non-blocking file appenders
- `tracing-subscriber` - Subscriber implementations and utilities

### Architecture

The logging system is implemented in `src/logging.rs` and provides:

1. **Structured Logging**: Uses `tracing` macros (`debug!`, `info!`, `warn!`, `error!`, `trace!`)
2. **Multiple Output Destinations**: File logging, stdout logging, and MCP server logging
3. **Dynamic Log Level Control**: Runtime log level changes via `set_log_level()`
4. **Non-blocking I/O**: Uses `tracing_appender::non_blocking` for performance

## Logging Initialization

### Location
Logging is initialized in `src/cli/mod.rs` in the `Cli::execute()` method (lines ~230-250):

```rust
let _log_guard = initialize_logging(LogArgs {
    log_level: match self.verbose > 0 {
        true => Some(
            match self.verbose {
                1 => Level::WARN,
                2 => Level::INFO,
                3 => Level::DEBUG,
                _ => Level::TRACE,
            }
            .to_string(),
        ),
        false => None,
    },
    log_to_stdout: std::env::var_os("Q_LOG_STDOUT").is_some() || self.verbose > 0,
    log_file_path: match subcommand {
        RootSubcommand::Chat { .. } => Some(logs_dir().expect("home dir must be set").join("qchat.log")),
        _ => None,
    },
    delete_old_log_file: false,
});
```

### Configuration Options

1. **Verbosity Levels** (via `-v` flag):
   - `-v`: WARN level
   - `-vv`: INFO level  
   - `-vvv`: DEBUG level
   - `-vvvv+`: TRACE level

2. **Environment Variables**:
   - `Q_LOG_LEVEL`: Sets log level directly (defined in `src/util/consts.rs:47`)
   - `Q_LOG_STDOUT`: Forces logging to stdout

3. **File Logging**: Only enabled for chat subcommand, writes to `qchat.log`

## Logging Patterns Used

### 1. Structured Tracing Macros (Primary Pattern)

**Most Common Pattern**: Using `tracing` macros with structured fields

Examples from `src/cli/chat/mod.rs`:
```rust
// Line 817: Debug with structured data
debug!(?request_metadata, "ctrlc received");

// Line 279: Info with structured field
info!(?conversation_id, "Generated new conversation id");

// Line 285: Warning with error context
tracing::warn!(?err, "Failed to check MCP configuration, defaulting to enabled");

// Line 846: Error with structured data
error!(?err, "An error occurred processing the current state");
```

**Key Characteristics**:
- Uses `?` syntax for Debug formatting of complex types
- Includes contextual fields alongside messages
- Consistent across the codebase

### 2. User-Facing Output (Secondary Pattern)

**Pattern**: Direct `println!` and `eprintln!` for user interaction

Examples from `src/cli/user.rs`:
```rust
// Lines 284-287: User authentication flow
println!();
println!("Confirm the following code in the browser");
println!("Code: {}", device_auth.user_code.bold());
println!();

// Lines 179-182: Logout confirmation
eprintln!("You are now logged out");
eprintln!(
    "Run {} to log back in to {PRODUCT_NAME}",
    format!("{CLI_BINARY_NAME} login").magenta()
);
```

**Usage Context**: Only for direct user communication, not debugging

### 3. Formatted Output (Tertiary Pattern)

**Pattern**: `color_print::cprintln!` for styled output

Example from `src/cli/user.rs:225`:
```rust
color_print::cprintln!("\n<em>Profile:</em>\n{}\n{}\n", profile.profile_name, profile.arn);
```

## Log Levels and Usage

### Distribution by Level

1. **`debug!`**: Most frequent - Used for detailed execution flow
   - Request/response tracking
   - State transitions
   - Tool validation steps
   - Internal data structures

2. **`info!`**: Moderate usage - Important events
   - Conversation initialization
   - Configuration changes
   - User actions

3. **`warn!`**: Limited usage - Non-fatal issues
   - Configuration fallbacks
   - Failed optional operations
   - Performance warnings

4. **`error!`**: Critical issues only
   - Request failures
   - Stream interruptions
   - Tool execution errors

5. **`trace!`**: Minimal usage - Finest detail level

## File Organization

### Log File Locations
- **Chat logs**: `~/.local/share/amazon-q/logs/qchat.log` (or platform equivalent)
- **MCP logs**: `~/.local/share/amazon-q/logs/mcp.log` (when MCP is active)
- **Log rotation**: Files are truncated when they exceed 10MB (`MAX_FILE_SIZE`)

### Log File Management
From `src/logging.rs:85-95`:
- Automatic directory creation
- File permission setting (0o600 on Unix)
- Size-based rotation
- Optional file deletion on startup

## Advanced Features

### 1. Runtime Log Level Changes
```rust
// From src/logging.rs:140-155
pub fn set_log_level(level: String) -> Result<String, Error> {
    info!("Setting log level to {level:?}");
    // ... implementation with reloadable handle
}
```

### 2. MCP Server Logging
Separate log stream for Model Context Protocol servers with trace-level filtering.

### 3. Non-blocking I/O
All file logging uses `tracing_appender::non_blocking` to prevent I/O blocking the main thread.

## Key Findings

### ✅ What the codebase DOES use:
1. **Structured logging** with `tracing` ecosystem
2. **Multiple log levels** with runtime configuration
3. **Non-blocking file appenders** for performance
4. **Environment variable configuration**
5. **Separate streams** for different purposes (main, MCP)

### ❌ What the codebase does NOT use:
1. **Simple `if(debug) println!()` patterns** - None found
2. **Basic `println!` for debugging** - Only used for user interaction
3. **Manual debug flags** - Uses proper log levels instead
4. **Synchronous file I/O** - All logging is non-blocking

### 🔧 Architecture Benefits:
1. **Performance**: Non-blocking I/O prevents logging from slowing execution
2. **Flexibility**: Runtime log level changes without restart
3. **Structure**: Rich contextual information in logs
4. **Separation**: Different log streams for different purposes
5. **Standards**: Uses industry-standard `tracing` ecosystem

## Recommendations for New Code

When adding logging to new code in this codebase:

1. **Use `tracing` macros**: `debug!`, `info!`, `warn!`, `error!`, `trace!`
2. **Include context**: Use `?field` syntax for structured data
3. **Choose appropriate levels**: Follow existing patterns for level selection
4. **Avoid `println!` for debugging**: Reserve for user-facing output only
5. **Consider performance**: Logging is already optimized, use freely at appropriate levels

## References

- **Main logging module**: `src/logging.rs`
- **Initialization**: `src/cli/mod.rs:230-250`
- **Environment variables**: `src/util/consts.rs:47`
- **Example usage**: `src/cli/chat/mod.rs` (extensive examples throughout)
