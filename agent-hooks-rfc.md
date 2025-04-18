- Feature Name: agent_hooks
- Start Date: 2025-04-10

# Summary

[summary]: #summary

This RFC proposes adding Agent hooks to Amazon Q CLI Chat, enabling users to define natural language cause-and-effect relationships that automatically trigger actions during query processing. These hooks allow for persistent automation of repetitive tasks without requiring explicit commands each time.

# Motivation

[motivation]: #motivation

Users often have recurring patterns in their workflows where certain actions should automatically follow specific events. For example:
- Updating README.md when code in a specific folder changes
- Running tests when a function is modified
- Generating documentation when new modules are added
- Enforcing code style or security checks when files are created or modified
- Automating Git workflows when specific events occur
- Integrating with project management tools like Jira to streamline development processes

Currently, users must remember to perform these actions manually or set up complex automation outside of Q CLI Chat. By integrating agent hooks directly into Q CLI Chat, we can:

1. Reduce cognitive load on users by automating repetitive tasks
2. Ensure consistency in workflows by automatically applying best practices
3. Leverage natural language processing to make automation accessible without complex scripting
4. Provide a seamless experience where Q CLI proactively assists based on user-defined patterns

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

## Defining Agent Hooks

Agent hooks are defined using natural language statements that describe a cause and an effect. The basic syntax is:

```
/hook create "When [cause], then [effect]"
```

For example:
```
/hook create "When any file in src/components is created or modified, then update README.md with the new component information"
```

## Listing and Managing Hooks

Users can list, view, edit, and delete their hooks:

```
/hook list
/hook show <hook-id>
/hook edit <hook-id> "When [new-cause], then [new-effect]"
/hook delete <hook-id>
```

## Hook Activation

Hooks are automatically activated when Q CLI detects that the cause condition has been met during normal operation. For example, if the user runs a command that modifies a file in `src/components`, Q CLI will:

1. Detect the file change event
2. Match it against registered hooks
3. Execute the effect action (updating README.md)
4. Notify the user that the hook was triggered

## Hook Notification and Control

Users can control how hooks are executed:

```
/hook config --notification=always|important-only|never
/hook config --auto-execute=always|ask|never
/hook disable <hook-id>
/hook enable <hook-id>
```

## Example Use Cases

1. **Documentation Maintenance**:
   ```
   /hook create "When code in lib/ changes, then update API documentation"
   ```

2. **Testing Workflow**:
   ```
   /hook create "When test files are modified, then run the affected tests"
   ```

3. **Project Standards**:
   ```
   /hook create "When new JavaScript files are created, then ensure they have proper header comments"
   ```

4. **Dependency Management**:
   ```
   /hook create "When package.json is modified, then check for security vulnerabilities"
   ```

5. **Cloud Resource Monitoring**:
   ```
   /hook create "When an SNS notification about EC2 instance termination is received, then update the inventory database"
   ```

6. **Terminal Adaptation**:
   ```
   /hook create "When terminal is resized, then adjust the display format of running commands"
   ```

7. **System Integration**:
   ```
   /hook create "When system goes into low power mode, then save all current work and suspend background tasks"
   ```

8. **Git Workflow Enhancement**:
   ```
   /hook create "When git pull is called, then review any merge conflicts and present them in a table with suggested actions"
   ```

9. **Jira Integration**:
   ```
   /hook create "When the Jira MCP is called to change the state of a ticket to Under Development, then create a new Git branch with the Jira ticket number and description based off latest main branch"
   ```

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

## System Architecture

The agent hooks system consists of several components:

1. **Hook Parser**: Converts natural language hook definitions into structured representations
2. **Event Monitor**: Detects file system and command events that might trigger hooks
3. **Matcher Engine**: Determines if an event satisfies a hook's cause condition
4. **Action Executor**: Performs the effect action when a hook is triggered
5. **Persistence Layer**: Stores hook definitions across sessions

### Hook Representation

Internally, hooks are represented as:

```rust
struct Hook {
    id: String,
    cause: Condition,
    effect: Action,
    enabled: bool,
    created_at: DateTime,
    last_triggered: Option<DateTime>,
}

enum Condition {
    FileEvent(FileEventCondition),
    CommandEvent(CommandEventCondition),
    TimeEvent(TimeEventCondition),
    SystemEvent(SystemEventCondition),
    TerminalEvent(TerminalEventCondition),
    CloudEvent(CloudEventCondition),
    CustomEvent(CustomEventCondition),
    GitEvent(GitEventCondition),
    JiraEvent(JiraEventCondition),
    // Other condition types
}

enum Action {
    FileModification(FileModificationAction),
    CommandExecution(CommandExecutionAction),
    Notification(NotificationAction),
    // Other action types
}
```

### Natural Language Processing

The hook parser uses NLP techniques to extract:

1. **Event type**: File creation, modification, deletion, command execution, etc.
2. **Target**: Files, directories, or commands affected
3. **Constraints**: Additional conditions like file types or content patterns
4. **Action**: What should happen when the condition is met

For example, parsing "When any Python file in src/ is modified, then run unit tests" would extract:
- Event: File modification
- Target: Python files in src/
- Action: Run unit tests

### Event Detection

The system monitors relevant events through:

1. File system watchers for file events (using efficient event-based APIs rather than polling)
2. Command interception for command events
3. External event sources:
   - Operating system events (process start/stop, network changes, etc.)
   - Terminal events (resize, focus change, session start/end)
   - Cloud service notifications (SNS, EventBridge, webhooks)
   - Custom event publishers (via a plugin API)
4. Periodic checks for time-based events

### Hook Optimization

To minimize performance impact, hooks are:

1. Pre-parsed and compiled into optimized data structures at creation time
2. Indexed by event type and target patterns for quick lookup
3. Selectively activated based on workspace context
4. Cached with relevant metadata to avoid redundant processing
5. Prioritized based on usage patterns and importance

### Hook Execution Flow

1. User performs an action (e.g., edits a file)
2. Event Monitor detects the action and creates an Event object
3. Matcher Engine uses indexed lookup to quickly identify potentially matching hooks
4. For each candidate hook, detailed matching is performed against pre-compiled conditions
5. For each matching hook:
   a. If auto-execute is enabled, Action Executor performs the effect
   b. Otherwise, user is prompted for confirmation
6. Results and notifications are displayed to the user

### Persistence

Hooks are stored in a JSON file in the Q CLI Chat configuration directory:

```
~/.aws/amazonq/hooks.json
```

## Integration Points

The agent hooks system integrates with:

1. **Q CLI Chat Command System**: For registering hook management commands
2. **File System Monitoring**: For detecting file events
3. **Command Execution**: For intercepting and executing commands
4. **NLP Pipeline**: For parsing natural language hook definitions
5. **Notification System**: For alerting users about hook triggers
6. **External Event Sources**:
   - Operating system event APIs
   - Terminal emulator events
   - AWS service event streams (SNS, EventBridge)
   - Custom event publishers via plugin architecture
7. **Developer Tools**:
   - Git operations and events
   - IDE actions and events
   - Build system outputs
8. **Project Management Tools**:
   - Jira API integration
   - GitHub/GitLab issue events
   - Trello board updates

# Drawbacks

[drawbacks]: #drawbacks

1. **Complexity**: Adds significant complexity to the Q CLI Chat system
2. **Performance Impact**: Continuous monitoring for hook triggers could impact performance. This could be mitigated by:
   - Pre-parsing hooks into optimized data structures at definition time
   - Using event-based triggers rather than polling
   - Implementing selective monitoring that only watches relevant files/directories
   - Batching file system checks during idle periods
   - Allowing users to configure monitoring frequency for non-critical hooks
3. **Ambiguity**: Natural language processing may misinterpret user intentions
4. **Security Concerns**: Automatic execution of actions could pose security risks
5. **Learning Curve**: Users need to learn how to effectively define hooks

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why This Design?

1. **Natural Language Interface**: Makes automation accessible to users without scripting knowledge
2. **Integration with Existing Q CLI Chat**: Leverages the existing NLP capabilities of Q
3. **Flexible Trigger System**: Supports various types of events and conditions
4. **User Control**: Provides options for notification and execution preferences

## Alternatives Considered

1. **Git Hooks**: Limited to Git operations and requires scripting knowledge
2. **External Automation Tools**: Requires users to learn and set up separate systems
3. **Rule-Based System with Formal Syntax**: More precise but less user-friendly
4. **Event-Driven Programming Model**: More powerful but significantly more complex

## Impact of Not Doing This

Without agent hooks:
1. Users continue to manually perform repetitive tasks
2. Inconsistent application of best practices
3. Higher cognitive load on users to remember workflow steps
4. Missed opportunity to differentiate Q CLI Chat with proactive assistance

# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. How complex can the natural language definitions be while maintaining reliable parsing?
2. What is the performance impact of continuous monitoring for hook triggers?
   - How can we optimize file system watching to minimize resource usage?
   - What strategies can be employed to reduce the overhead of hook evaluation?
   - How can we balance responsiveness with system resource consumption?
   - Should hooks have different priority levels that affect monitoring frequency?
3. How should conflicts between multiple matching hooks be resolved?
4. What security measures are needed to prevent malicious hook definitions?
5. How should hooks handle errors during execution?
6. Should hooks be shareable between users or projects?
7. What telemetry data should be collected to improve the hook system while respecting user privacy?

# Future possibilities

[future-possibilities]: #future-possibilities

1. **Hook Marketplace**: Allow users to share and discover useful hooks
2. **Hook Templates**: Predefined hooks for common workflows
3. **Hook Chaining**: Allow hooks to trigger other hooks in sequence
4. **Context-Aware Hooks**: Hooks that consider the broader context of user actions
5. **Learning Hooks**: Hooks that adapt based on user behavior and feedback
6. **Team Hooks**: Shared hooks for team workflows and standards
7. **Integration with CI/CD**: Connect hooks to external CI/CD pipelines
8. **Visual Hook Editor**: GUI for creating and managing complex hooks
9. **Hook Analytics**: Track hook usage and effectiveness
10. **Cross-Project Hooks**: Apply hooks across multiple projects or repositories
11. **Telemetry-Based Optimization**: Collect anonymous usage data to improve hook suggestions and performance
12. **Telemetry-Driven Recommendations**: Suggest new hooks based on observed patterns in user workflows
13. **Effectiveness Metrics**: Measure and report on time saved and productivity gains from hook automation
14. **Adaptive Triggers**: Refine hook activation conditions based on telemetry data about false positives/negatives
15. **Community Insights**: Aggregate anonymized telemetry to identify most valuable hook patterns across users
16. **External Event Integration**: Support for third-party event sources through a standardized event adapter API
