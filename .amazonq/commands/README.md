# Custom slash commands
Custom slash commands allow you to define frequently-used prompts as Markdown files that Claude Code can execute. Commands are organized by scope (project-specific or personal) and support namespacing through directory structures.

## Syntax
```
/<prefix>:<command-name> [arguments]
```
Parameters
Parameter	Description
`<prefix>` Command scope (project for project-specific, user for personal)
`<command-name>` Name derived from the Markdown filename (without .md extension)
`[arguments]` Optional arguments passed to the command

## Command types

Project commands
Commands stored in your repository and shared with your team.

Location: .claude/commands/
Prefix: /project:

In the following example, we create the /project:optimize command:


```
# Create a project command
mkdir -p .claude/commands
echo "Analyze this code for performance issues and suggest optimizations:" > .claude/commands/optimize.md
```

Personal commands
Commands available across all your projects.

Location: ~/.claude/commands/
Prefix: /user:

In the following example, we create the /user:security-review command:


```
# Create a personal command
mkdir -p ~/.claude/commands
echo "Review this code for security vulnerabilities:" > ~/.claude/commands/security-review.md
```

Features

Namespacing
Organize commands in subdirectories to create namespaced commands.

Structure: <prefix>:<namespace>:<command>

For example, a file at .claude/commands/frontend/component.md creates the command /project:frontend:component


Arguments
Pass dynamic values to commands using the $ARGUMENTS placeholder.

For example:


```
# Command definition
echo 'Fix issue #$ARGUMENTS following our coding standards' > .claude/commands/fix-issue.md

# Usage
> /project:fix-issue 123
```

Bash command execution
Execute bash commands before the slash command runs using the ! prefix. The output is included in the command context.

For example:


```
---
allowed-tools: Bash(git add:*), Bash(git status:*), Bash(git commit:*)
description: Create a git commit
---

## Context

- Current git status: !`git status`
- Current git diff (staged and unstaged changes): !`git diff HEAD`
- Current branch: !`git branch --show-current`
- Recent commits: !`git log --oneline -10`

## Your task

Based on the above changes, create a single git commit.
```

File references
Include file contents in commands using the @ prefix to reference files.

For example:

```
# Reference a specific file
Review the implementation in @src/utils/helpers.js

# Reference multiple files
Compare @src/old-version.js with @src/new-version.js

Thinking mode
Slash commands can trigger extended thinking by including extended thinking keywords.
```

File format
Command files support:

- Markdown format (.md extension)
- YAML frontmatter for metadata:
  - allowed-tools: List of tools the command can use
  - description: Brief description of the command
- Prompt instructions as the main content
