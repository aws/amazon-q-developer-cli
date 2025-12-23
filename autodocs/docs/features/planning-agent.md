---
doc_meta:
  validated: 2025-12-22
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: feature
  title: Planning Agent
  description: Built-in agent that transforms ideas into structured implementation plans with requirements gathering and task breakdown
  keywords: [planning, agent, plan, requirements, tasks, workflow]
  related: [slash-plan, agent-configuration]
---

# Planning Agent

The Planning Agent is a specialized built-in agent that helps transform ideas into structured implementation plans. It guides you through requirements gathering, research, and creates detailed task breakdowns before switching to execution mode.

## Getting Started

**Accessing the Planning Agent**

1. **Keyboard Shortcut**: Press `Shift + Tab` to toggle between planning and execution modes
1. **Slash Command**: Use `/plan` to switch to the planning agent
```
> /plan
Switched to the Kiro [plan] agent.
Transform any idea into fully working code. What do you want to build today?

[plan] > 
```
3. **With Immediate Prompt**: Use `/plan` with your idea to start planning immediately
```
> /plan Build a REST API for user authentication
```

**Visual Indicators:**
- `[plan]` indicator in your status
- Welcome message when switching modes

## Planning Workflow

### 1. Requirements Gathering
The planner guides you through structured questions to refine your initial idea.
```
[plan] > I want to build a todo app

I understand you want to build a todo app. Let me help you plan this implementation.

**My Understanding:** You want to create a todo application for managing tasks.

Let me gather some requirements to create a detailed plan:

[1]: What platform should this todo app target?
a. **Web Application** - Browser-based with HTML/CSS/JavaScript
b. **Mobile App** - Native iOS/Android or cross-platform
c. **Desktop App** - Electron, Tauri, or native desktop
d. **CLI Tool** - Command-line interface
e. **Other** - Provide your own answer

[2]: What's the primary use case for this todo app?
a. **Personal Task Management** - Individual productivity
b. **Team Collaboration** - Shared tasks and assignments
c. **Project Management** - Complex workflows with dependencies
d. **Other** - Describe your specific needs

(Use the chat to answer any subset: eg., "1=a, 2=b" or provide your own answers)
```

**Key Features:**
- **Structured Questions**: Clear, numbered questions with multiple choice options
- **Flexible Responses**: Answer individual questions or provide custom responses
- **Iterative Process**: Builds understanding through multiple rounds of questions
- **Context Awareness**: Adapts follow-up questions based on your answers

### 2. Research & Analysis
The planner explores your codebase and researches relevant technologies.

**Capabilities:**
- **Code Exploration**: Uses `code`, `grep`, and `glob` tools to understand existing code
- **Technology Research**: Identifies relevant frameworks, libraries, and patterns
- **Architecture Analysis**: Reviews existing project structure and conventions

### 3. Implementation Plan
Creates a detailed, step-by-step implementation plan with clear objectives.
```
**Implementation Plan - Todo CLI Command**

**Problem Statement:**
Add todo management to existing Kiro CLI for task tracking.

**Requirements:**
- CLI commands for CRUD operations
- Local SQLite storage
- Priority and due date support

**Task Breakdown:**

Task 1: Create database schema and models
- Define Todo struct with required fields
- Create database migration for todos table
- Demo: Can create and query todos in database

Task 2: Implement CLI command structure  
- Add todo subcommand with add/list/complete operations
- Demo: CLI accepts todo commands and shows help

Task 3: Add advanced features
- Implement due dates and priority sorting
- Demo: Complete todo system with all features
```

**Task Requirements:**
- **Clear Objectives**: Each task has a specific, measurable goal
- **Implementation Guidance**: General approach and key considerations
- **Demo Description**: Working functionality that can be demonstrated

### 4: Plan Approval & Handoff
Before switching to execution, the planner seeks your approval.

```
[plan] > Does this plan look good, or would you like me to adjust anything?

> The plan looks great! Let's implement it.

*Planning complete!*
Ready to exit [plan] agent to start your implementation? [y/n]:

> y
```

**Handoff Process:**
1. **Plan Confirmation**: User approves the implementation plan
2. **Switch Confirmation**: Interactive prompt to confirm switching to execution
3. **Automatic Transition**: Switches back to previous agent
4. **Context Transfer**: Passes the complete plan to the execution agent

## Key Features

**Read-Only Design**: Planning agent cannot modify files or execute commands - keeps focus on planning rather than implementation.
  * **Restricted Operations:**
    1. **File Writing**: Cannot create or modify files (`fs_write` denied)
    1. **Command Execution**: Limited to read-only bash commands
    1. **MCP Tools**: Excluded from most external tools
  * **Allowed Operations:**
    1. **File Reading**: Full access to explore codebase
    1. **Code Intelligence**: LSP operations for understanding code structure
    1. **Search Operations**: `grep`, `glob` for code exploration
    1. **Research Tools**: Web search and documentation access

## Best Practices

- **Use for complex tasks**: Most valuable for multi-step implementations
- **Engage with questions**: Answer structured questions thoughtfully  
- **Let it explore**: Allow planner to analyze your existing codebase
- **Review plans**: Ensure plan matches expectations before handoff
- **Iterate as needed**: Continue refining until plan is clear

## Troubleshooting

**Cannot switch to execution**: System defaults to `kiro_default` agent if no previous agent exists.
**Planning interrupted**: Use `/plan` to restart or continue with partial requirements.
**Need to make changes**: Switch to execution mode (`Shift + Tab`) for file modifications.

## Appendix
### Example - Complex Planning Workflow

```
> /plan Add user authentication to my web app

[plan] > I understand you want to add user authentication to your web application.

**My Understanding:** You want to implement user authentication functionality 
for an existing web application.

[1]: What authentication method do you prefer?
a. **Email/Password** - Traditional username and password
b. **OAuth** - Google, GitHub, or other OAuth providers  
c. **Magic Links** - Passwordless email-based authentication
d. **Multi-factor** - Combination of methods with 2FA

> 1=a

[plan] > Great! Email/password authentication it is.

[2]: What's your current tech stack?
a. **React + Node.js** - JavaScript full-stack
b. **Next.js** - React with server-side rendering
c. **Django/Flask** - Python web framework
d. **Other** - Please specify

> 2=d, I'm using Rust with Axum framework

[plan] > Perfect! Rust with Axum is an excellent choice for secure authentication.

*Researching Axum authentication patterns...*
*Exploring your existing codebase structure...*

**Implementation Plan - User Authentication System**
[Detailed plan follows...]

Does this plan look good, or would you like me to adjust anything?

> Looks perfect!

Ready to exit [plan] agent to start your implementation? [y/n]: y

[default] > Implement this plan:
[Plan content transferred to execution agent]
```