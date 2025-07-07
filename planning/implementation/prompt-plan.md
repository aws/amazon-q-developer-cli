# Implementation Prompt Plan

## Checklist
- [x] Prompt 1: Set up command structure and subcommand infrastructure
- [x] Prompt 2: Implement session state management for server enable/disable
- [x] Prompt 3: Create server reload manager and core lifecycle operations
- [x] Prompt 4: Implement reload command with server restart functionality
- [ ] Prompt 5: Add enable and disable commands with session state integration
- [ ] Prompt 6: Implement status and list commands with enhanced display
- [ ] Prompt 7: Add comprehensive error handling and user feedback
- [ ] Prompt 8: Integrate configuration reloading and validation
- [ ] Prompt 9: Add progress display and loading indicators
- [ ] Prompt 10: Implement backward compatibility and default behavior
- [ ] Prompt 11: Add comprehensive testing and validation
- [ ] Prompt 12: Wire everything together and final integration

## Prompts

### Prompt 1: Set up command structure and subcommand infrastructure
Extend the existing `/mcp` command structure to support subcommands while maintaining backward compatibility. Create the foundational command parsing infrastructure that will support reload, enable, disable, status, and list operations.

1. Modify `McpArgs` in `/crates/chat-cli/src/cli/chat/cli/mcp.rs` to include optional subcommands
2. Create `McpSubcommand` enum with all required subcommand variants
3. Create individual argument structures for each subcommand (ReloadArgs, EnableArgs, etc.)
4. Implement the execute method to route to appropriate subcommand handlers
5. Ensure backward compatibility - when no subcommand is provided, maintain current display behavior
6. Add basic unit tests to verify command parsing works correctly

Focus on clean architecture with clear separation between command parsing and business logic. The subcommand handlers should be placeholder implementations that return success for now.

### Prompt 2: Implement session state management for server enable/disable
Create the session state management system that tracks which servers are enabled or disabled for the current chat session only, without modifying configuration files.

1. Create `SessionServerState` struct to track session-only server state changes
2. Add methods for enabling/disabling servers and checking server state
3. Integrate session state into `ToolManager` or create a separate state manager
4. Implement logic to determine effective server state (config + session overrides)
5. Add state isolation to ensure different chat sessions don't interfere
6. Create unit tests for state management logic

Ensure the session state correctly overrides configuration file settings and provides clean separation between persistent and temporary state changes.

### Prompt 3: Create server reload manager and core lifecycle operations
Implement the core server lifecycle management functionality that handles stopping, starting, and restarting MCP servers with proper resource cleanup.

1. Create `ServerReloadManager` struct with methods for server lifecycle operations
2. Implement server validation to check if servers exist in configuration
3. Add server stop functionality that cleanly terminates processes and removes tools
4. Implement server start functionality that creates new clients and establishes connections
5. Add tool registry cleanup and re-registration logic
6. Create error types specific to reload operations
7. Add unit tests for lifecycle operations with mocked dependencies

Focus on atomic operations that either succeed completely or fail without leaving the system in an inconsistent state.

### Prompt 4: Implement reload command with server restart functionality
Create the reload command implementation that performs full server restart with configuration refresh and connection re-establishment.

1. Implement `ReloadArgs::execute()` method with complete reload logic
2. Add configuration file re-reading to pick up any changes
3. Integrate with server lifecycle manager for stop/start operations
4. Implement proper error handling with user-friendly messages
5. Add progress indication during reload operations
6. Ensure tool registry is properly updated after reload
7. Add integration tests with actual server restart scenarios

The reload should handle the complete cycle: validate server, stop process, clear tools, re-read config, start new process, register tools, and update display.

### Prompt 5: Add enable and disable commands with session state integration
Implement the enable and disable commands that modify server state for the current session only, integrating with the session state management system.

1. Implement `EnableArgs::execute()` to enable disabled servers for current session
2. Implement `DisableArgs::execute()` to disable enabled servers for current session
3. Add validation to prevent enabling already-enabled servers and vice versa
4. Integrate with session state management to track changes
5. Add server startup/shutdown logic for enable/disable operations
6. Implement proper error handling and user feedback
7. Add tests for enable/disable functionality and state persistence

Ensure that enable/disable operations properly start/stop servers and update the tool registry while maintaining session-only state changes.

### Prompt 6: Implement status and list commands with enhanced display
Create status and list commands that show server information with session state awareness, maintaining consistency with existing `/mcp` command display format.

1. Implement `StatusArgs::execute()` to show detailed server information
2. Implement `ListArgs::execute()` to list all servers with current state
3. Integrate session state to show effective server status (config + session overrides)
4. Maintain existing display format and styling for consistency
5. Add server state indicators (enabled/disabled/loading/failed)
6. Include tool count and last reload time information
7. Add tests for display formatting and state representation

The display should clearly indicate which servers are affected by session-only state changes versus configuration file settings.

### Prompt 7: Add comprehensive error handling and user feedback
Implement robust error handling with clear user feedback that follows established patterns in the Q CLI system.

1. Create comprehensive error types for all failure scenarios
2. Implement error display with consistent terminal formatting
3. Add validation with helpful error messages showing available options
4. Implement graceful degradation for partial failures
5. Add user guidance for common error scenarios
6. Ensure all errors allow chat session to continue
7. Add tests for error handling and message formatting

Focus on providing actionable error messages that help users understand what went wrong and how to fix it.

### Prompt 8: Integrate configuration reloading and validation
Add configuration file reloading capability that detects and applies configuration changes without requiring application restart.

1. Implement configuration file re-reading for workspace and global configs
2. Add configuration validation before applying changes
3. Integrate configuration reload with server restart operations
4. Handle configuration file errors gracefully
5. Add support for detecting configuration changes
6. Implement proper error handling for configuration issues
7. Add tests for configuration reloading scenarios

Ensure configuration reloading is atomic and doesn't leave servers in inconsistent states if configuration is invalid.

### Prompt 9: Add progress display and loading indicators
Implement progress display and loading indicators that reuse existing infrastructure to show reload operations status to users.

1. Integrate with existing loading display system for consistency
2. Add progress indicators for reload operations
3. Implement spinner animations and status messages
4. Add timing information for operations
5. Handle both interactive and non-interactive modes appropriately
6. Ensure progress display doesn't interfere with chat functionality
7. Add tests for progress display in different modes

The progress display should provide clear feedback about operation status while maintaining the existing user experience patterns.

### Prompt 10: Implement backward compatibility and default behavior
Ensure the extended `/mcp` command maintains full backward compatibility with existing usage patterns and scripts.

1. Implement default behavior when no subcommand is provided
2. Ensure existing `/mcp` usage continues to work unchanged
3. Add comprehensive backward compatibility tests
4. Validate that existing scripts and workflows are unaffected
5. Implement proper help text and usage information
6. Add migration path documentation if needed
7. Test with existing configuration files and setups

The implementation must not break any existing functionality while adding new capabilities.

### Prompt 11: Add comprehensive testing and validation
Create a comprehensive test suite that covers all functionality, error conditions, and edge cases for the hot reload feature.

1. Add unit tests for all command implementations
2. Create integration tests with actual MCP servers
3. Add error scenario testing for all failure modes
4. Implement state consistency testing
5. Add concurrent operation testing
6. Create user experience validation tests
7. Add performance and resource cleanup tests

Ensure test coverage includes both happy path and error scenarios, with particular attention to state consistency during failures.

### Prompt 12: Wire everything together and final integration
Complete the implementation by integrating all components, ensuring proper initialization, and validating the complete feature works end-to-end.

1. Integrate all components into the main chat system
2. Ensure proper initialization of session state and reload managers
3. Add final integration testing with complete workflows
4. Validate error handling works correctly in the full system
5. Test backward compatibility with existing functionality
6. Add final documentation and usage examples
7. Perform end-to-end validation of all use cases

Focus on ensuring all components work together seamlessly and the feature integrates cleanly with the existing Q CLI system without disrupting other functionality.
