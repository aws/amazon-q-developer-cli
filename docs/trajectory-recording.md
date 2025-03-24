# Trajectory Recording System

The trajectory recording system in Amazon Q Developer for command line allows you to record, visualize, and restore the steps of a conversation with Amazon Q. This feature is useful for debugging, sharing, and understanding how Amazon Q processes your requests.

## Enabling Trajectory Recording

To enable trajectory recording, start Amazon Q with the `--trajectory` flag:

```bash
q chat --trajectory
```

You can specify a custom directory for storing trajectory data:

```bash
q chat --trajectory --trajectory-dir ~/my-trajectories
```

To automatically generate visualizations after each response:

```bash
q chat --trajectory --auto-visualize
```

## Available Commands

Once in a chat session with trajectory recording enabled, you can use the following commands:

### Status

Check the current status of trajectory recording:

```
/trajectory status
```

This will show whether trajectory recording is enabled and display the current configuration.

### Creating Checkpoints

Create a checkpoint of the current conversation state:

```
/trajectory checkpoint create <label>
```

Replace `<label>` with a descriptive name for your checkpoint.

### Listing Checkpoints

List all available checkpoints:

```
/trajectory checkpoint list
```

This will show the ID, label, and creation timestamp for each checkpoint.

### Restoring from Checkpoints

Restore the conversation from a checkpoint:

```
/trajectory checkpoint restore <id>
```

Replace `<id>` with the ID of the checkpoint you want to restore.

### Generating Visualizations

Generate an HTML visualization of the current trajectory:

```
/trajectory visualize
```

This will create an HTML file in the trajectory directory and display the path.

### Enabling/Disabling Recording

Enable trajectory recording:

```
/trajectory enable
```

Disable trajectory recording:

```
/trajectory disable
```

### Help

Show help for trajectory commands:

```
/trajectory help
```

Show help for checkpoint commands:

```
/trajectory checkpoint help
```

## Understanding Visualizations

The generated HTML visualizations show the flow of a conversation, including:

1. User instructions
2. Agent reasoning
3. Tool uses and their results
4. Agent responses

Each step is connected to show the flow of the conversation, and checkpoints are highlighted.

## Best Practices

1. **Create checkpoints at important stages**: Before making significant changes or trying different approaches, create a checkpoint so you can easily return to that state.

2. **Use descriptive labels**: When creating checkpoints, use descriptive labels that will help you remember what each checkpoint represents.

3. **Visualize complex conversations**: For complex interactions with multiple tool uses, generate visualizations to better understand the flow.

4. **Preserve context when needed**: For debugging purposes, you can enable full context preservation with `--preserve-full-context` to capture more details.

## Limitations

1. Trajectory recording increases memory usage and disk space requirements.
2. Very long conversations with many tool uses may result in large visualization files.
3. Restoring from checkpoints may not preserve all external state (e.g., changes made to the filesystem).

## Troubleshooting

If trajectory commands are not working:

1. Make sure you started Amazon Q with the `--trajectory` flag.
2. Check the status with `/trajectory status` to verify it's enabled.
3. If disabled, enable it with `/trajectory enable`.
4. Ensure you have write permissions to the trajectory directory.

If visualizations are not generating:

1. Check that the trajectory directory exists and is writable.
2. Try manually generating a visualization with `/trajectory visualize`.
3. Check for error messages in the output.
