- Feature Name: q_cli_logging
- Start Date: 2025-04-29

# Summary

[summary]: #summary

This RFC proposes adding a logging feature to the Amazon Q CLI tool that will record user prompts, commands, responses, and related metadata in a structured format. The feature will be opt-in, enabled via a command-line flag or in-session command, and will provide users with commands to view, filter, and manage their interaction logs.

# Motivation

[motivation]: #motivation

Users of the Amazon Q CLI tool often need to:
- Review their past interactions with the tool
- Track their usage patterns over time
- Reference previous solutions or responses
- Share their interaction history with team members
- Analyze their productivity and tool usage

Currently, there is no built-in way to record and review these interactions, forcing users to manually copy and paste important information or lose their history entirely when closing the terminal. This feature addresses this gap by providing a structured, searchable log of all interactions that users can easily access and manage.

The expected outcomes are:
1. Improved user productivity through easy access to past interactions
2. Better collaboration by enabling sharing of interaction histories
3. Enhanced learning by allowing users to review their usage patterns
4. Increased user satisfaction by preserving valuable information

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

## Enabling Logging

There are two ways to enable logging for a Q CLI session:

### 1. At Startup

Use the `--enable-logging` flag when starting the tool:

```bash
q --enable-logging
```

### 2. During a Session

Enable logging during an active session using the `/log enable` command:

```
/log enable
```

Similarly, you can disable logging during a session:

```
/log disable
```

When logging is enabled, a new log file is created for the current session in the standard system log location for your operating system:
- macOS: `~/Library/Logs/AmazonQ/`
- Linux: `~/.local/share/amazon-q/logs/`
- Windows: `%LOCALAPPDATA%\Amazon\Q\Logs\`

## Viewing Logs

Once logging is enabled, you can view your logs using the `/log` command within the Q CLI:

```
/log show
```

This displays the 10 most recent log entries in the current session. Each entry includes:
- Timestamp
- User prompt/command
- Compact summary of the response
- Time taken for the response to generate
- Context files used (if any)

## Log Command Options

The `/log` command supports several options:

```
/log show                 # Show the 10 most recent entries
/log show --all           # Show all entries in the current session
/log show count=N         # Show N most recent entries 
/log --tail=N             # Show the last N entries
/log --head=N             # Show the first N entries
/log show --desc          # Show entries in descending order of timestamp
/log delete               # Delete logs for the current session only
/log enable               # Enable logging for the current session
/log disable              # Disable logging for the current session
/log show --only-user-prompts  # Show only user prompts, not system commands
```

## Example Usage

Here's an example of enabling logging and using the log commands:

1. Start Q CLI with logging enabled:
   ```bash
   q --enable-logging
   ```

2. Use Q CLI normally:
   ```
   > How do I create an S3 bucket?
   To create an S3 bucket using the AWS CLI, you can use the following command:
   ...
   ```

3. View your recent interactions:
   ```
   > /log show
   [2025-04-29 10:15:22] User prompt: How do I create an S3 bucket?
   Response time: 1.2s
   Context: None
   Response summary: Provided steps to create an S3 bucket using AWS CLI and console
   ```

4. View only user prompts:
   ```
   > /log show --only-user-prompts
   [Shows only entries from user prompts, not system commands]
   ```

5. Enable logging mid-session:
   ```
   > /log enable
   Logging has been enabled for the current session.
   ```

## Impact on Codebase Maintenance

The logging feature is designed with maintainability in mind:
- Clear separation of concerns with dedicated components for logging, session management, and command handling
- Consistent file format (JSONL) that's both human-readable and machine-parseable
- Standardized log locations following OS conventions
- Automatic log truncation to prevent excessive disk usage

This design makes the feature easy to maintain and extend in the future, with minimal impact on the core Q CLI functionality.

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

## Architecture

The Q CLI Logging Feature consists of four main components:

1. **LogManager**: Core component responsible for managing logging functionality
2. **LogEntry**: Represents a single log entry
3. **LogCommandHandler**: Processes log-related commands

### Component Interfaces

#### LogManager

```rust
struct LogManager {
    // Fields
    session_id: String,
    log_file_path: PathBuf,
    enabled: bool,
}

impl LogManager {
    fn initialize_logging(enable_logging: bool) -> Result<Self>;
    fn enable_logging() -> Result<()>;
    fn disable_logging() -> Result<()>;
    fn is_logging_enabled() -> bool;
    fn log_interaction(prompt: &str, response: &str, response_time: f64, context_files: &[String], is_user_prompt: bool) -> Result<()>;
    fn get_log_entries(count: Option<usize>, show_all: bool, tail: Option<usize>, head: Option<usize>, desc: bool, only_user_prompts: bool) -> Result<Vec<LogEntry>>;
    fn delete_current_session_logs() -> Result<bool>;
    fn check_and_truncate_log_file() -> Result<()>;
}
```

#### LogEntry

```rust
struct LogEntry {
    id: String,
    timestamp: DateTime<Utc>,
    prompt: String,
    response_summary: String,
    response_time_seconds: f64,
    context_files: Vec<String>,
    session_id: String,
    is_user_prompt: bool,
}

impl LogEntry {
    fn new(prompt: &str, response: &str, response_time: f64, context_files: &[String], session_id: &str, is_user_prompt: bool) -> Self;
    fn to_json(&self) -> Result<String>;
    fn from_json(json_str: &str) -> Result<Self>;
    fn generate_summary(response: &str) -> String;
}
```

#### LogCommandHandler

```rust
struct LogCommandHandler {
    log_manager: LogManager,
}

impl LogCommandHandler {
    fn new(log_manager: LogManager) -> Self;
    fn handle_command(command: &str, args: &[&str]) -> Result<String>;
    fn handle_show_command(args: &[&str]) -> Result<String>;
    fn handle_tail_command(count: usize) -> Result<String>;
    fn handle_head_command(count: usize) -> Result<String>;
    fn handle_delete_command() -> Result<String>;
    fn handle_enable_command() -> Result<String>;
    fn handle_disable_command() -> Result<String>;
}
```

### Data Storage

#### Log File Format

Logs are stored in JSONL (JSON Lines) format, with each line containing a single JSON object representing a log entry:

```json
{"id":"entry_001","timestamp":"2025-04-25T14:30:22Z","prompt":"How do I create an S3 bucket?","response_summary":"Provided steps to create an S3 bucket using AWS CLI and console","response_time_seconds":1.2,"context_files":[],"session_id":"a1b2c3d4","is_user_prompt":true}
{"id":"entry_002","timestamp":"2025-04-25T14:35:45Z","prompt":"/log show","response_summary":"Displayed log entries","response_time_seconds":0.1,"context_files":[],"session_id":"a1b2c3d4","is_user_prompt":false}
```

#### File Structure

The logging feature uses the following file structure:

```
[log_directory]/
├── sessions/
│   ├── q_session_YYYY-MM-DD_HH-MM-SS_[session_id].log
│   ├── q_session_YYYY-MM-DD_HH-MM-SS_[session_id].log
│   └── ...
└── current_session -> sessions/q_session_YYYY-MM-DD_HH-MM-SS_[session_id].log (symlink to current session)
```

Where `[log_directory]` is the platform-specific log directory.

### Implementation Details

#### Enabling Logging

When the `--enable-logging` flag is provided at startup or the `/log enable` command is used during a session:

1. The `LogManager` initializes with the new session ID and creates the log directory if it doesn't exist
2. A new log file is created for the session with the naming convention `q_session_YYYY-MM-DD_HH-MM-SS_[session_id].log`
3. A symlink to the current session log file is created/updated
4. The `enabled` flag in `LogManager` is set to `true`

Similarly, when the `/log disable` command is used:
1. The `enabled` flag in `LogManager` is set to `false`
2. No further interactions are logged until logging is enabled again

#### Logging Interactions

For each user interaction:

1. The `LogManager.log_interaction` method is called with the prompt, response, response time, context files, and a flag indicating whether it's a user prompt or system command
2. If logging is disabled, the method returns early without logging
3. A new `LogEntry` is created with a unique ID and the current timestamp
4. The response is summarized using the `LogEntry.generate_summary` method
5. The entry is serialized to JSON and appended to the log file
6. The log file size is checked, and truncation is performed if necessary

#### Log Truncation

When a log file exceeds 512MB:

1. The `LogManager.check_and_truncate_log_file` method is called
2. The file size is checked, and if it exceeds 512MB:
   - The target size is calculated (e.g., 90% of max size)
   - The file is read from an appropriate position
   - The first partial line is discarded to ensure entry integrity
   - The remaining content is written back to the file
   - A truncation notice is added at the beginning

#### Command Handling

When a `/log` command is entered:

1. The command is parsed to extract the subcommand and arguments
2. The appropriate method in `LogCommandHandler` is called:
   - `handle_show_command` for `/log show` (with optional `--all`, `--desc`, or `--only-user-prompts` flags)
   - `handle_tail_command` for `/log --tail N`
   - `handle_head_command` for `/log --head N`
   - `handle_delete_command` for `/log delete`
   - `handle_enable_command` for `/log enable`
   - `handle_disable_command` for `/log disable`
3. For show commands, the method retrieves the requested log entries from `LogManager`, applying filters like `only_user_prompts` if specified
4. The entries are formatted for display and returned to the user

### Error Handling

The feature implements comprehensive error handling:

1. **Log Directory Creation Failure**
   - Error: Unable to create log directory
   - Response: Log error to console, disable logging for the session
   - Recovery: Attempt to use a fallback directory in the user's home directory

2. **Log File Write Failure**
   - Error: Unable to write to log file
   - Response: Log error to console, continue operation without logging
   - Recovery: Attempt to recreate log file or use a new file

3. **Log File Read Failure**
   - Error: Unable to read log file
   - Response: Display error message to user
   - Recovery: Continue operation with available data or empty result

4. **Log File Size Limit Exceeded**
   - Error: Log file size exceeds 512MB
   - Response: Automatically truncate log file
   - Recovery: None needed, handled automatically

5. **Invalid Command Arguments**
   - Error: User provides invalid arguments to log commands
   - Response: Display usage information
   - Recovery: None needed, user must correct input

## Integration with Existing Codebase

The logging feature integrates with the existing Q CLI codebase by:

1. Adding a new `--enable-logging` flag to the command-line argument parser
2. Initializing the logging components when the flag is present
3. Adding hooks to log interactions at appropriate points in the code, with flags to distinguish between user prompts and system commands
4. Registering the `/log` command handler with the existing command processing system, including the new `enable`, `disable`, and `--only-user-prompts` options

This integration is designed to be minimally invasive, with the logging functionality contained in its own module and only interacting with the main codebase through well-defined interfaces.

# Drawbacks

[drawbacks]: #drawbacks

There are several potential drawbacks to implementing this feature:

1. **Increased Complexity**: Adding logging functionality increases the complexity of the codebase, which could make maintenance more challenging.

2. **Disk Space Usage**: Logs can consume significant disk space over time, especially for power users. While we implement truncation at 512MB, this could still be an issue for users with limited storage.

3. **Performance Impact**: Writing logs to disk for each interaction could potentially impact performance, especially on systems with slow storage.

4. **Privacy Concerns**: Storing user interactions could raise privacy concerns, as sensitive information might be inadvertently logged. This is mitigated by making logging opt-in and storing logs locally only.

5. **Maintenance Burden**: The feature will require ongoing maintenance to ensure compatibility with future changes to the Q CLI tool.

6. **Cross-Platform Complexity**: Implementing proper log storage across different operating systems adds complexity and potential for platform-specific bugs.

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why this design?

This design was chosen because it:

1. **Balances Simplicity and Functionality**: The design provides comprehensive logging capabilities while maintaining a simple user interface.

2. **Follows Platform Conventions**: By storing logs in standard system locations, the feature integrates well with existing system tools and user expectations.

3. **Minimizes Resource Usage**: The JSONL format and line-based truncation approach minimize disk space usage and processing overhead.

4. **Preserves User Privacy**: The opt-in approach and local-only storage respect user privacy while still providing the benefits of logging.

5. **Supports Future Extensions**: The modular design allows for easy addition of new features like log compression, advanced filtering, and analytics.

## Alternatives Considered

### 1. Persistent Logging (Always Enabled)

Instead of opt-in logging, we could enable logging by default for all sessions.

**Rationale for not choosing**: This would raise privacy concerns and consume disk space unnecessarily for users who don't need logging.

### 2. Database Storage

Instead of flat files, we could store logs in a lightweight database like SQLite.

**Rationale for not choosing**: This would add a dependency on a database library and increase complexity without significant benefits for the current use cases.

### 3. Remote Logging

We could offer an option to sync logs to a remote storage service.

**Rationale for not choosing**: This would raise significant privacy concerns and add unnecessary complexity for what is primarily a local tool.

### 4. Log Rotation Instead of Truncation

Instead of truncating large log files, we could implement log rotation with multiple files.

**Rationale for not choosing**: While this would preserve more historical data, it would consume more disk space and add complexity to the log management system.

## Impact of Not Doing This

If we don't implement this feature:

1. Users will continue to lose their interaction history when closing the terminal
2. Users will need to manually copy and paste important information
3. There will be no way to track usage patterns or review past interactions
4. Users may resort to third-party solutions that might be less integrated or secure

# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. **Command Syntax**: Should we use `/log` as the command prefix, or would another syntax be more consistent with existing commands?

2. **Log Retention Policy**: Should we implement an automatic cleanup policy for older logs, or leave this to the user?

3. **Response Summary Generation**: What's the optimal approach for generating compact summaries of responses? Should we use a fixed character limit, extract the first few sentences, or use a more sophisticated approach?

4. **Performance Impact**: How significant is the performance impact of logging, especially on systems with slow storage? Do we need to implement buffering or asynchronous writes?

5. **Cross-Session Queries**: Should we support querying logs across multiple sessions, or keep the focus on the current session only?

6. **Security Considerations**: Should we implement any additional security measures for log files, such as encryption or redaction of sensitive information?

# Future possibilities

[future-possibilities]: #future-possibilities

There are several natural extensions to this feature that could be implemented in the future:

1. **Log Compression**: Automatically compress older log files to save disk space.

2. **Advanced Filtering**: Add support for filtering logs by date range, keywords, or other criteria.

3. **Log Export**: Allow users to export logs to different formats (CSV, PDF, etc.) for sharing or analysis.

4. **Log Analytics**: Implement basic analytics to show usage patterns, common queries, etc.

5. **Cross-Session Queries**: Add support for querying logs across multiple sessions.

6. **Log Synchronization**: Allow users to synchronize logs across multiple devices (with appropriate privacy controls).

7. **Selective Logging**: Allow users to mark certain interactions as private (not to be logged).

8. **Log Annotations**: Allow users to add notes or tags to log entries for easier reference.

9. **Integration with Other Tools**: Provide APIs or hooks for other tools to access and analyze logs.

10. **Contextual Recall**: Use logged interactions to improve the context and relevance of future responses.

These possibilities demonstrate the potential for the logging feature to evolve beyond simple record-keeping into a powerful tool for productivity, learning, and collaboration.
