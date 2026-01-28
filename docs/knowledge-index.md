# Documentation Index

This index provides an overview of all documentation files in the kiro-cli project.

## Core Documentation

### [introduction.md](./introduction.md)
---
title: Introduction
description: Welcome page for the supplementary KIRO CLI Developer documentation
---

### [SUMMARY.md](./SUMMARY.md)
---
title: Summary
description: Table of contents and navigation structure for the documentation
---

### [agent-format.md](./agent-format.md)
---
title: Agent Format
description: Complete reference for agent configuration JSON files including name, description, prompt, mcpServers, tools, toolAliases, allowedTools, toolsSettings, resources, hooks, and model fields
---

### [agent-file-locations.md](./agent-file-locations.md)
---
title: Agent File Locations
description: Where to place agent configuration files - local (.kiro/agents/) vs global (~/.kiro/agents/) agents and precedence rules
---

### [default-agent-behavior.md](./default-agent-behavior.md)
---
title: Default Agent Behavior
description: Agent selection priority and fallback hierarchy when no specific agent is configured
---

## Built-in Tools & Features

### [built-in-tools.md](./built-in-tools.md)
---
title: Built-in Tools
description: Reference for all built-in tools - execute_bash, fs_read, fs_write, introspect, report_issue, knowledge, thinking, todo_list, use_aws, and use_subagent with configuration options
---

### [introspect-tool.md](./introspect-tool.md)
---
title: Introspect Tool
description: Self-awareness tool that answers questions about KIRO CLI features, commands, and functionality using official documentation
---

### [hooks.md](./hooks.md)
---
title: Hooks
description: Execute custom commands at specific trigger points - agentSpawn, userPromptSubmit, preToolUse, postToolUse, and stop hooks for security validation, logging, and automation
---

## Experimental Features

### [experiments.md](./experiments.md)
---
title: Experimental Features
description: Toggle experimental features with /experiment command - checkpointing, context usage percentage, knowledge, thinking, tangent mode, delegate, and TODO lists
---

### [tangent-mode.md](./tangent-mode.md)
---
title: Tangent Mode
description: Create conversation checkpoints to explore side topics without disrupting main conversation flow
---

### [knowledge-management.md](./knowledge-management.md)
---
title: Knowledge Management
description: Persistent knowledge base functionality - store, search, and manage contextual information across chat sessions with semantic search
---

### [todo-lists.md](./todo-lists.md)
---
title: TODO Management
description: Persistent TODO list management with /todos command - view, resume, and manage TODO lists created during chat sessions
---

## Agents & Subagents

### [subagent.md](./subagent.md)
---
title: Subagent
description: Delegate complex tasks to specialized subagents running in parallel with isolated context using use_subagent tool
---

### [planning-agent.md](./planning-agent.md)
---
title: Planning Agent
description: Built-in agent that transforms ideas into structured implementation plans with requirements gathering and task breakdowns
---

## Code Intelligence

### [code-intelligence.md](./code-intelligence.md)
---
title: Code Intelligence
description: LSP integration for semantic code understanding - search_symbols, find_references, goto_definition, get_document_symbols, rename_symbol, and get_diagnostics
---

## Web & External

### [web-search.md](./web-search.md)
---
title: Web Search
description: Access current information from the internet with web_search and web_fetch tools for research and fact verification
---

### [mcp-registry.md](./mcp-registry.md)
---
title: MCP Registry
description: MCP server access control for Pro-tier customers using IAM Identity Center with organization-managed registries
---

## Session & Migration

### [session-management.md](./session-management.md)
---
title: Session Management
description: Automatic chat session saving and resumption - resume, resume-picker, and session listing commands
---

### [legacy-profile-to-agent-migration.md](./legacy-profile-to-agent-migration.md)
---
title: Profile to Agent Migration
description: Migrate global profiles to agents directory and handle MCP configuration migration
---

## Design Documents

### [design/slash-commands-acp-design.md](./design/slash-commands-acp-design.md)
---
title: Slash Commands ACP Design
description: Architecture for bidirectional slash command system bridging Rust agents crate with TypeScript TUI using Agent Client Protocol
---

## Task Documents

### [tasks/long-running-subagents.md](./tasks/long-running-subagents.md)
---
title: Long-Running Subagents
description: Implementation plan for subagent feature in ACP runtime with user-initiated background subagent sessions
---

### [tasks/event-sourced-session-persistence.md](./tasks/event-sourced-session-persistence.md)
---
title: Event-Sourced Session Persistence
description: Implementation plan for event-sourced state management with append-only log and session/load functionality
---

### [tasks/api-client-enum-refactoring.md](./tasks/api-client-enum-refactoring.md)
---
title: ApiClient Enum Refactoring
description: Add enum-based architecture to ApiClient supporting real API calls and IPC-based mocking for E2E tests
---

### [tasks/e2e-testing-implementation.md](./tasks/e2e-testing-implementation.md)
---
title: E2E Testing Implementation
description: Comprehensive E2E testing with real processes, mocked LLM API responses, and IPC state querying
---
