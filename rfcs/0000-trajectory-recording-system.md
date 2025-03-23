- Feature Name: trajectory_recording_system
- Start Date: 2025-03-23

# Summary

[summary]: #summary

This RFC proposes a trajectory recording system for the Amazon Q CLI that allows users to record, visualize, and manage the history of agent actions during conversations. The system includes features for creating checkpoints, restoring from checkpoints, and generating visual representations of the agent's decision-making process.

# Motivation

[motivation]: #motivation

When users interact with Amazon Q through the CLI, they often need to understand how the agent arrived at specific responses or recommendations. Currently, there's no built-in way to:

1. Track the sequence of reasoning steps and tool uses that led to a particular response
2. Save the state of a conversation at critical points
3. Restore previous conversation states to explore alternative paths
4. Visualize the agent's decision-making process

This feature addresses these needs by providing a comprehensive trajectory recording system that captures the agent's internal reasoning, tool usage, and responses. It enables users to:

- Debug unexpected agent behaviors
- Share agent interactions with team members
- Create educational materials showing how the agent works
- Experiment with different conversation paths from saved checkpoints
- Analyze patterns in agent reasoning and tool usage

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

## Using Trajectory Recording

To use the trajectory recording system, users start Amazon Q with the `--trajectory` flag:

```bash
q chat --trajectory --trajectory-dir ~/my-trajectories
```

This enables recording of all agent actions during the conversation. Users can also add the `--auto-visualize` flag to automatically generate and open visualizations after each agent response:

```bash
q chat --trajectory --trajectory-dir ~/my-trajectories --auto-visualize
```

## Available Commands

Once trajectory recording is enabled, users can access the following commands:

### Checkpoint Management

```
/trajectory checkpoint create <label>    # Create a checkpoint with the given label
/trajectory checkpoint list              # List all available checkpoints
/trajectory checkpoint restore <id>      # Restore a conversation from a checkpoint
/trajectory checkpoint help              # Show checkpoint help
```

### Visualization

```
/trajectory visualize                    # Generate visualization of the current trajectory
```

### Recording Control

```
/trajectory enable                       # Enable trajectory recording
/trajectory disable                      # Disable trajectory recording
/trajectory status                       # Show current trajectory recording status
```

### Help

```
/trajectory help                         # Show trajectory help
```

## Example Workflow

1. Start Amazon Q with trajectory recording enabled:
   ```bash
   q chat --trajectory --trajectory-dir ~/my-trajectories
   ```

2. Begin a conversation with Amazon Q about a technical topic.

3. Create a checkpoint before asking a complex question:
   ```
   /trajectory checkpoint create before_complex_question
   ```

4. Ask the complex question and review the response.

5. Generate a visualization to see how the agent arrived at its answer:
   ```
   /trajectory visualize
   ```

6. If you want to try a different approach, restore from the checkpoint:
   ```
   /trajectory checkpoint restore 1
   ```

7. Ask a different version of the question and compare the results.

## Visualization Format

The visualization is presented as an HTML file with a git-like history view:
- A table layout with columns for graph, message, and timestamp
- Visual graph with nodes and connecting lines
- Color-coded nodes for different step types (user input, agent reasoning, tool use, response)
- Expandable details for each step
- Special formatting for commands vs. regular user input

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

## Architecture

The trajectory recording system consists of several key components:

1. **TrajectoryRecorder**: Records user instructions, agent reasoning, tool uses, tool results, and responses
2. **TrajectoryRepository**: Handles storage and retrieval of trajectory data
3. **TrajectoryVisualizer**: Generates HTML visualizations of the trajectory
4. **TrajectoryCommandHandler**: Processes trajectory-related commands
5. **ChatContext Integration**: Connects the trajectory system with the main chat flow

### Data Model

The trajectory is represented as a sequence of steps, where each step has:
- A unique identifier
- A timestamp
- A step type (user input, agent reasoning, tool use, tool result, response)
- Content specific to the step type
- Metadata for visualization

### File Structure

The implementation is organized into the following files:

- `~/workspace/amazon-q-developer-cli/crates/q_cli/src/cli/chat/trajectory/mod.rs`: Public API and configuration
- `~/workspace/amazon-q-developer-cli/crates/q_cli/src/cli/chat/trajectory/recorder.rs`: Core recording functionality
- `~/workspace/amazon-q-developer-cli/crates/q_cli/src/cli/chat/trajectory/repository.rs`: Storage and retrieval
- `~/workspace/amazon-q-developer-cli/crates/q_cli/src/cli/chat/trajectory/visualizer.rs`: HTML visualization generation
- `~/workspace/amazon-q-developer-cli/crates/q_cli/src/cli/chat/trajectory/command_handler.rs`: Command processing

### Command Processing Flow

When a user enters a trajectory command:

1. The input is parsed in `ChatContext::handle_trajectory_command`
2. The command is delegated to `TrajectoryCommandHandler`
3. The handler processes the command and performs the requested action
4. Results are returned to the user

### Checkpoint System

Checkpoints are implemented by:

1. Capturing the current conversation state
2. Storing it with a user-provided label
3. Assigning a unique identifier
4. Providing retrieval by ID

Restoring from a checkpoint:
1. Retrieving the stored conversation state
2. Replacing the current conversation with the stored one
3. Notifying the user of successful restoration

### Visualization Generation

The visualization process:

1. Retrieves the recorded trajectory steps
2. Generates an HTML document with CSS styling
3. Creates a git-like history view with nodes and connecting lines
4. Saves the HTML to the specified directory
5. Opens the file in the default browser (if auto-visualize is enabled)

## Implementation Details

### Recording Agent Actions

The recorder hooks into key points in the agent's processing pipeline:

1. User input is captured before being sent to the agent
2. Agent reasoning steps are recorded during processing
3. Tool usage is intercepted and recorded
4. Tool results are captured
5. Agent responses are recorded before being displayed to the user

### Browser Tab Management

To prevent opening multiple browser tabs when auto-visualization is enabled:

1. A `browser_opened` flag tracks if a browser tab has been opened
2. The first visualization opens a new browser tab
3. Subsequent visualizations update the HTML file without opening new tabs
4. Users can refresh the existing tab to see updates

### Tool Use Descriptions

Tool uses are recorded with enhanced descriptions:

1. Tool name and operation are extracted
2. Parameters are formatted in a human-readable way
3. File operations show filename and command type
4. Command executions show a preview of the command

# Drawbacks

[drawbacks]: #drawbacks

1. **Performance Impact**: Recording all agent actions could potentially slow down the agent's response time, especially for complex conversations.

2. **Storage Requirements**: Storing complete trajectories with tool inputs and outputs could require significant disk space for long conversations.

3. **Implementation Complexity**: The system requires hooks into various parts of the agent's processing pipeline, which increases code complexity and maintenance burden.

4. **User Interface Complexity**: Adding another set of commands increases the learning curve for users.

5. **Privacy Considerations**: Recording detailed agent actions might capture sensitive information that users didn't intend to save.

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why This Design?

This design was chosen because it:

1. Provides a comprehensive view of agent actions while being minimally invasive to the existing codebase
2. Uses a command-based interface that's consistent with other CLI features
3. Separates concerns into distinct components (recording, storage, visualization, command handling)
4. Offers flexibility through optional flags and commands
5. Creates visualizations that are easy to understand and share

## Alternative Approaches Considered

### 1. Log-based approach

Instead of a structured recording system, we could simply enhance logging to capture agent actions.

**Rationale for not choosing**: Logs are harder to parse, visualize, and interact with. They don't provide the checkpoint functionality that's valuable for experimentation.

### 2. External monitoring tool

We could create a separate tool that monitors the agent's actions from outside.

**Rationale for not choosing**: An external tool would have limited visibility into the agent's internal reasoning and would require additional setup steps for users.

### 3. Real-time visualization only

We could focus solely on real-time visualization without recording or checkpoints.

**Rationale for not choosing**: This would miss the opportunity to provide checkpoint functionality and would limit users' ability to review past interactions.

## Impact of Not Implementing

Without this feature:

1. Users will continue to lack visibility into how the agent arrives at its responses
2. Debugging agent behavior will remain difficult
3. Users won't be able to save and restore conversation states
4. Sharing agent interactions will be limited to copying text
5. Educational opportunities to understand agent reasoning will be missed

# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. **Performance Optimization**: How can we minimize the performance impact of recording, especially for long conversations?

2. **Storage Management**: Should we implement automatic cleanup of old trajectory data? If so, what policies should govern retention?

3. **Visualization Enhancements**: What additional visualization features would be most valuable to users?

4. **Privacy Controls**: What controls should users have over what gets recorded and for how long?

5. **Integration with Other Tools**: How should trajectory recording interact with other debugging and development tools?

# Future possibilities

[future-possibilities]: #future-possibilities

## Real-time Visualization Updates

Implement a local web server approach for real-time visualization updates:
- Create a simple HTTP server that serves the visualization
- Use WebSockets to push updates to the browser when new steps are added
- Eliminate the need for manual refreshes

## Enhanced Checkpoint System

Improve the checkpoint user experience:
- Add visual indicators in the UI when checkpoints are created
- Provide more detailed information about checkpoints in the list view
- Implement a confirmation dialog before restoring from checkpoints

## Trajectory Comparison

Add the ability to compare two different trajectories side by side:
- Highlight differences in agent reasoning
- Show alternative tool uses
- Measure efficiency metrics between approaches

## Trajectory Sharing

Create a secure way to share trajectories with team members:
- Export/import functionality
- Redaction of sensitive information
- Integration with collaboration tools

## Machine Learning Integration

Use recorded trajectories to improve agent performance:
- Analyze patterns in successful interactions
- Identify areas where the agent consistently struggles
- Train models to improve reasoning paths

## IDE Integration

Extend trajectory visualization to work within IDEs:
- VSCode extension for in-editor visualization
- JetBrains plugin for trajectory viewing
- Integration with debugging tools
