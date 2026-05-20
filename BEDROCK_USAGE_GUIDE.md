# Amazon Q Developer CLI - Bedrock Integration Usage Guide

## Overview

This guide covers how to use Amazon Q Developer CLI with Amazon Bedrock as the backend. The Bedrock integration allows you to use Claude models directly through your AWS account while maintaining all existing Q Developer CLI functionality and tools.

## Prerequisites

- AWS credentials configured (via `~/.aws/credentials` or environment variables)
- Access to Amazon Bedrock with Claude models enabled in your AWS account
- Amazon Q Developer CLI installed

## Quick Start

### 1. Enable Bedrock Mode

```bash
q config bedrock true
```

### 2. Set Your AWS Region

```bash
q config region us-west-2
```

### 3. Configure Your Model

List available models in your account:
```bash
q chat
/model
```

Set a specific model using settings:
```bash
q settings bedrock.model "us.anthropic.claude-sonnet-4-20250514-v1:0"
```

### 4. Start Chatting

```bash
q chat
```

No login required when Bedrock mode is enabled!

## Common Commands Quick Reference

```bash
# Configuration
q config bedrock true                    # Enable Bedrock
q config region us-west-2                # Set region
q config max-tokens 8192                 # Set max output tokens
q config temperature 0.7                 # Set temperature
q config thinking true                   # Enable thinking mode

# System Prompts
q config system-prompt add "name" "prompt text"
q config system-prompt enable "name"
q config system-prompt default
q config system-prompt list

# Model Selection
q config bedrock-model                  # Interactive model picker

# Settings
q settings list                          # View all settings
q settings bedrock.model "model-id"      # Set model directly

# Chat
q chat                                   # Start chat
/model                                   # Select model (in chat)
```

## Configuration Commands

### Bedrock Mode Toggle

Enable Bedrock backend:
```bash
q config bedrock true
```

Disable Bedrock backend (returns to Q Developer):
```bash
q config bedrock false
```

### Region Configuration

Set AWS region for Bedrock API calls:
```bash
q config region us-east-1
q config region us-west-2
q config region eu-west-1
q config region us-gov-west-1
q config region us-iso-west-1
q config region us-isob-east-1 (when available)
```

Supports all AWS commercial regions, GovCloud, ISO, and ISO-B regions.

### Model Selection

**Option 1: Interactive command-line selection (recommended)**
```bash
q config bedrock-model
```
This queries your AWS account for available models and lets you select one interactively.

**Option 2: Interactive in-chat selection**
```bash
q chat
/model
```
This queries your AWS account for available Claude models and lets you select one during a chat session.

**Option 3: Direct configuration**
```bash
q settings bedrock.model "anthropic.claude-3-5-sonnet-20241022-v2:0"
```

**Common Model IDs:**
- Claude 4 Sonnet (inference profile): `us.anthropic.claude-sonnet-4-20250514-v1:0`
- Claude 3.5 Sonnet v2: `anthropic.claude-3-5-sonnet-20241022-v2:0`
- Claude 3.5 Sonnet: `anthropic.claude-3-5-sonnet-20240620-v1:0`
- Claude 3 Opus: `anthropic.claude-3-opus-20240229-v1:0`

### Max Output Tokens

Set the maximum number of tokens in the model's response (up to 200,000):
```bash
q config max-tokens 4096
q config max-tokens 8192
q config max-tokens 16384
```

Default: 4096 tokens

**Note:** This controls the output length, not the input context window. Different models have different maximum token limits.

### Thinking Mode

Enable extended thinking mode (automatically sets temperature to 1.0):
```bash
q config thinking true
```

Disable thinking mode:
```bash
q config thinking false
```

When thinking mode is enabled, the model uses extended reasoning and temperature is locked at 1.0.

### Temperature Control

Set temperature (0.0 to 1.0):
```bash
q config temperature 0.7
q config temperature 0.3
q config temperature 1.0
```

**Note:** Temperature can only be configured when thinking mode is disabled. If thinking mode is enabled, temperature is automatically set to 1.0.

### Custom System Prompts

**Add a new system prompt:**
```bash
q config system-prompt add "python-expert" "You are a Python expert. Focus on Pythonic code and best practices."
q config system-prompt add "security" "You are a security expert focused on identifying vulnerabilities."
q config system-prompt add "concise" "Be extremely brief. One sentence maximum."
```

**List all system prompts:**
```bash
q config system-prompt list
```

Output shows all prompts with an `(active)` marker for the currently enabled prompt:
```
Custom system prompts:
  - python-expert (active)
    You are a Python expert. Focus on Pythonic code and best pra...
  - security
    You are a security expert focused on identifying vulnerabili...
  - concise
    Be extremely brief. One sentence maximum.
```

**Enable a system prompt:**
```bash
q config system-prompt enable python-expert
```

**Default system prompt:**
```bash
q config system-prompt disable
```

**Delete a system prompt:**
```bash
q config system-prompt delete concise
```

If you delete the active prompt, it will be automatically deactivated.

**Default behavior:** When no custom system prompt is active, the model uses its default system prompt.

## Settings Management

### View All Bedrock Settings

```bash
q settings list --all | grep bedrock
```

### View Configured Settings

```bash
q settings list
```

### Get a Specific Setting

```bash
q settings bedrock.enabled
q settings bedrock.region
q settings bedrock.model
```

### Set a Setting Directly

```bash
q settings bedrock.enabled true
q settings bedrock.region "us-west-2"
q settings bedrock.model "anthropic.claude-3-5-sonnet-20241022-v2:0"
```

### Delete a Setting

```bash
q settings --delete bedrock.temperature
```

## Complete Configuration Example

Here's a complete setup workflow:

```bash
# 1. Enable Bedrock mode
q config bedrock true

# 2. Set your region
q config region us-west-2

# 3. Set context window
q config context-window 200000

# 4. Configure temperature
q config temperature 0.7

# 5. Add custom system prompts
q config system-prompt add "code-reviewer" "You are an expert code reviewer. Focus on best practices, security, and performance."
q config system-prompt enable code-reviewer

# 6. Start chatting
q chat

# 7. Inside chat, select your model
/model
```

## Tool Support

All existing Q Developer CLI tools work seamlessly with Bedrock:

- **File operations:** `fs_read`, `fs_write`
- **AWS operations:** `use_aws` (S3, EC2, Lambda, etc.)
- **Bash execution:** `execute_bash`
- **And all other tools**


## Authentication

**No login required!** When Bedrock mode is enabled, authentication is bypassed. The CLI uses your AWS credentials directly.

To check your configuration:
```bash
q settings list
```

## Switching Between Q Developer and Bedrock

**Switch to Bedrock:**
```bash
q config bedrock true
```

**Switch back to Q Developer:**
```bash
q config bedrock false
q login  # Re-authenticate with Q Developer
```

All your Bedrock settings are preserved when you switch back and forth.

## Supported Models

The `/model` command and `q config bedrock-model` dynamically query your AWS Bedrock account and display all available **text generation models**. This includes:

- **Anthropic:** Claude 4, Claude 3.5, Claude 3 (Opus, Sonnet, Haiku)
- **Amazon:** Titan Text models
- **Meta:** Llama 2, Llama 3 models
- **Mistral AI:** Mistral and Mixtral models
- **Cohere:** Command models
- **AI21 Labs:** Jurassic models

### Model Filtering

The CLI automatically filters models to show only those suitable for text generation:

- ✅ Shows only text generation models
- ✅ Shows only ACTIVE models (excludes unreleased or deprecated models)

### Inference Profiles

Models with `us.` prefix (e.g., `us.anthropic.claude-sonnet-4-20250514-v1:0`) are **inference profiles** that provide:

- Better availability through cross-region routing
- Automatic failover if a region is unavailable
- Recommended for production use

The CLI automatically resolves models to inference profiles when available. You can use either the direct model ID or the inference profile ID.

**Usage:**
```bash
q config bedrock-model    # Interactive selection with all available models
q chat
/model                    # In-chat selection
```


## Advanced Usage

### Using Inference Profiles

For Claude 4 models, use inference profiles for better availability:
```bash
q settings bedrock.model "us.anthropic.claude-sonnet-4-20250514-v1:0"
```

### Multiple System Prompts for Different Tasks

Create task-specific prompts:
```bash
q config system-prompt add "debugging" "You are a debugging expert. Focus on root cause analysis."
q config system-prompt add "documentation" "You are a technical writer. Create clear, concise documentation."
q config system-prompt add "architecture" "You are a software architect. Focus on system design and scalability."
```

Switch between them as needed:
```bash
q config system-prompt enable debugging
# Work on debugging...

q config system-prompt enable documentation
# Write documentation...
```

### Temperature Strategies

- **Creative tasks:** `q config temperature 0.9`
- **Balanced:** `q config temperature 0.7`
- **Deterministic/factual:** `q config temperature 0.3`
- **Extended thinking:** `q config thinking true` (locks to 1.0)

### Large Context Windows

For working with large codebases:
```bash
q config context-window 200000
```

This allows the model to see more of your code at once.

## Configuration Reference

| Setting | Command | Values | Default |
|---------|---------|--------|---------|
| Bedrock Mode | `q config bedrock <value>` | `true`, `false` | `false` |
| Region | `q config region <region>` | Any AWS region | `us-east-1` |
| Model | `q settings bedrock.model <id>` | Model ID string | None |
| Context Window | `q config context-window <size>` | Number (e.g., 8192) | 4096 |
| Thinking Mode | `q config thinking <value>` | `true`, `false` | `false` |
| Temperature | `q config temperature <value>` | 0.0 to 1.0 | 0.7 |
| Active Prompt | `q config system-prompt enable <name>` | Prompt name | None |

## Tips

1. **Start simple** - Enable Bedrock, set region, start chatting
2. **Use inference profiles** - Models are automatically resolved to inference profiles when available
3. **Create system prompts** - Build a library for different tasks, switch with `enable` or return to default
4. **Adjust temperature** - Lower (0.3) for factual, higher (0.9) for creative
5. **Use interactive model selection** - Run `q config bedrock-model` to browse and select models
6. **Monitor costs** - Bedrock usage bills to your AWS account

## Best Practices

1. **Start with defaults:** Enable Bedrock mode and set your region, then adjust other settings as needed
2. **Use inference profiles:** For Claude 4 models, use inference profiles (us.* prefix) for better availability
3. **Match context window to task:** Use larger windows for complex codebases, smaller for focused tasks
4. **Create reusable prompts:** Build a library of system prompts for different types of work
5. **Test temperature settings:** Different tasks benefit from different temperature values
6. **Monitor costs:** Bedrock usage is billed to your AWS account - monitor in AWS Cost Explorer

## Getting Help

View all config commands:
```bash
q config --help
```

View system-prompt commands:
```bash
q config system-prompt --help
```

View settings commands:
```bash
q settings --help
```

## Learn More

- **AWS Bedrock:** https://aws.amazon.com/bedrock/
- **Claude Models:** https://docs.anthropic.com/claude/docs

## Summary

The Bedrock integration provides:
- ✅ All existing Q Developer CLI tools and features
- ✅ No separate authentication required
- ✅ Flexible configuration options
- ✅ Custom system prompts
- ✅ Temperature and context window control
- ✅ Extended thinking mode support

