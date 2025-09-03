# Introspect Tool

The introspect tool provides Q CLI with self-awareness, automatically answering questions about Q CLI's features, commands, and functionality using official documentation.

## How It Works

The introspect tool activates automatically when you ask Q CLI questions like:
- "How do I save conversations with Q CLI?"
- "What experimental features does Q CLI have?"
- "Can Q CLI read files?"

## What It Provides

- **Command Help**: Real-time help for all slash commands (`/save`, `/load`, etc.)
- **Documentation**: Access to README, built-in tools, experiments, and feature guides
- **Settings**: All configuration options and how to change them
- **GitHub Links**: Direct links to official documentation for verification

## Important Limitations

**Hallucination Risk**: Despite safeguards, the AI may occasionally provide inaccurate information or make assumptions. **Always verify important details** using the GitHub documentation links provided in responses.

## Usage Examples

```
> How do I save conversations with Q CLI?
You can save conversations using `/save` or `/save name`.
Load them later with `/load`.

> What experimental features does Q CLI have?
Q CLI offers Tangent Mode and Thinking Mode. 
Use `/experiment` to enable them.

> Can Q CLI read and write files?
Yes, Q CLI has fs_read, fs_write, and execute_bash tools
for file operations.
```

## Auto-Tangent Mode

Enable automatic tangent mode for Q CLI help questions:

```bash
q settings introspect.tangentMode true
```

This keeps help separate from your main conversation.

## Best Practices

1. **Be explicit**: Ask "How does Q CLI handle files?" not "How do you handle files?"
2. **Verify information**: Check the GitHub links provided in responses
3. **Use proper syntax**: Reference commands with `/` (e.g., `/save`)
4. **Enable auto-tangent**: Keep help isolated from main conversations

## Configuration

```bash
# Enable auto-tangent for introspect questions
q settings introspect.tangentMode true
```

## Related Features

- **Tangent Mode**: Isolate help conversations
- **Experiments**: Enable experimental features with `/experiment`
