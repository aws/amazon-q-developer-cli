# Amazon Q Agent Trajectory Visualization

This document provides an overview of the trajectory recording and visualization feature in Amazon Q Developer CLI.

## Overview

The trajectory recording system allows users to:

1. Record agent actions during a conversation
2. Create checkpoints of conversation state
3. Restore from checkpoints
4. Visualize the trajectory of agent actions

This feature helps users understand, debug, and share agent interactions by providing insights into how the agent processes requests and makes decisions.

## Getting Started

To use the trajectory recording feature, start Amazon Q with the `--trajectory` flag:

```bash
q chat --trajectory
```

You can also specify a custom directory for storing trajectory data:

```bash
q chat --trajectory --trajectory-dir ~/my-trajectories
```

To automatically generate visualizations after each agent response:

```bash
q chat --trajectory --auto-visualize
```

## Commands

The trajectory feature provides the following commands:

### Basic Commands

- `/trajectory status` - Show current trajectory recording status
- `/trajectory enable` - Enable trajectory recording
- `/trajectory disable` - Disable trajectory recording
- `/trajectory help` - Show trajectory help

### Visualization

- `/trajectory visualize` - Generate visualization of the current trajectory

### Checkpoints

- `/trajectory checkpoint create <label>` - Create a checkpoint with the given label
- `/trajectory checkpoint list` - List all available checkpoints
- `/trajectory checkpoint restore <id>` - Restore a conversation from a checkpoint
- `/trajectory checkpoint help` - Show checkpoint help

## Visualization Features

The trajectory visualization provides a comprehensive view of the agent's actions:

### Git-like History View

The visualization uses a git-like history view with:

- A graph column showing nodes and connecting lines
- A message column showing the content of each step
- A timestamp column showing when each step occurred

### Step Types

The visualization distinguishes between different types of steps:

- **User Input**: User instructions to the agent
- **Reasoning**: Agent's internal reasoning process
- **Tool Use**: Tools used by the agent to accomplish tasks
- **Tool Results**: Results returned by tools
- **Response**: Agent's responses to the user
- **Checkpoint**: Saved conversation states

### Interactive Elements

The visualization includes interactive elements:

- **Show Details** button to expand detailed information about each step
- Color-coded tags for different step types
- Expandable sections for tool parameters and results

## Example Workflow

1. Start Amazon Q with trajectory recording enabled:
   ```bash
   q chat --trajectory --auto-visualize
   ```

2. Ask a question that requires tool use:
   ```
   > List files in my current directory and tell me which ones are Rust source files
   ```

3. The agent will:
   - Record your instruction
   - Record its reasoning
   - Use tools (e.g., fs_read to list files)
   - Record tool results
   - Provide a response

4. A visualization will automatically open in your browser showing the complete trajectory

5. Create a checkpoint to save the current state:
   ```
   > /trajectory checkpoint create file-listing
   ```

6. Continue the conversation or restore from the checkpoint later:
   ```
   > /trajectory checkpoint restore file-listing
   ```

## Troubleshooting

### Visualization Not Opening

If the visualization doesn't open automatically:

1. Check that your browser allows automatic opening of files
2. Try manually opening the visualization file:
   ```bash
   open ~/q-agent-trajectory/trajectory.html
   ```
3. Ensure you have the necessary permissions to write to the trajectory directory

### Recording Not Working

If trajectory recording isn't working:

1. Verify that you started Amazon Q with the `--trajectory` flag
2. Check the status with `/trajectory status`
3. If disabled, enable it with `/trajectory enable`

## Technical Details

The trajectory recording system consists of several components:

- **Recorder**: Records user instructions, agent reasoning, tool uses, and responses
- **Repository**: Handles storage and retrieval of trajectory data
- **Visualizer**: Generates HTML visualizations of the trajectory
- **Command Handler**: Processes trajectory commands

Data is stored in a structured format that captures:

- The sequence of steps in the conversation
- Parent-child relationships between steps
- Detailed information about each step
- Checkpoints for restoring conversation state

## Future Enhancements

Planned enhancements for the trajectory visualization feature:

1. Real-time visualization updates without requiring browser refreshes
2. More interactive features like filtering and searching
3. Export options for sharing trajectories
4. Integration with other debugging tools
5. Performance optimizations for large trajectories
