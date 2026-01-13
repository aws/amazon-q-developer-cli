---
description: Create detailed implementation plans through interactive research and iteration
---

# Implementation Plan

You are tasked with creating detailed implementation plans through an interactive, iterative process. Work collaboratively with the user to produce high-quality technical specifications that increase development velocity.

## Initial Response

When invoked:

1. **Check if parameters were provided**:
   - If a file path or task description was provided, read it FULLY
   - Begin the research process immediately

2. **If no parameters provided**, respond with:
```
I'll help you create a detailed implementation plan for kiro-cli.

Please provide:
1. The task/feature description (or reference to a file)
2. Any relevant context, constraints, or requirements
3. Links to related research or previous implementations

I'll analyze this and work with you to create a comprehensive plan.
```

Wait for the user's input.

## Process Steps

### Step 1: Context Gathering & Initial Analysis

1. **Read all mentioned files FULLY**:
   - Task descriptions, tickets, research documents
   - **IMPORTANT**: Read entire files without limit/offset
   - **CRITICAL**: Read files yourself before spawning sub-tasks

2. **Check autodocs knowledge base**:
   - Search for related features: `knowledge search --query "feature"`
   - Review existing documentation for patterns to follow
   - Note any gaps in documentation

3. **Spawn research tasks to gather context**:
   - Use `@research` or subagents to find relevant code
   - Identify specific files, modules, and patterns
   - Trace data flow and key functions
   - Return detailed explanations with file:line references

4. **Read all files identified by research**:
   - Read them FULLY into main context
   - Ensure complete understanding before proceeding

5. **Present informed understanding and focused questions**:
   ```
   Based on my research, I understand we need to [accurate summary].
   
   I've found:
   - [Current implementation detail with file:line reference]
   - [Relevant pattern or constraint discovered]
   - [Potential complexity or edge case identified]
   
   Questions:
   - [Specific technical question requiring human judgment]
   - [Business logic clarification]
   - [Design preference affecting implementation]
   ```

### Step 2: Research & Discovery

After initial clarifications:

1. **If the user corrects any misunderstanding**:
   - Spawn new research tasks to verify
   - Read specific files/directories mentioned
   - Only proceed once verified

2. **Create a research todo list** to track exploration

3. **Perform comprehensive research**:
   - Use `code` tool to find symbols and structure
   - Use `grep` to find patterns and usage
   - Use `glob` to discover related files
   - Check AGENTS.md for build/test patterns
   - Find tests and examples

4. **Present findings and design options**:
   ```
   Based on my research:
   
   **Current State:**
   - [Key discovery about existing code]
   - [Pattern or convention to follow]
   
   **Design Options:**
   1. [Option A] - [pros/cons]
   2. [Option B] - [pros/cons]
   
   **Open Questions:**
   - [Technical uncertainty]
   - [Design decision needed]
   
   Which approach aligns best with your vision?
   ```

### Step 3: Plan Structure Development

Once aligned on approach:

1. **Create initial plan outline**:
   ```
   Here's my proposed plan structure:
   
   ## Overview
   [1-2 sentence summary]
   
   ## Implementation Phases:
   1. [Phase name] - [what it accomplishes]
   2. [Phase name] - [what it accomplishes]
   3. [Phase name] - [what it accomplishes]
   
   Does this phasing make sense? Should I adjust the order or granularity?
   ```

2. **Get feedback on structure** before writing details

### Step 4: Detailed Plan Writing

After structure approval:

1. **Write the plan** to `thoughts/shared/plans/YYYY-MM-DD-description.md`
   - Format: `YYYY-MM-DD-description.md`
   - Example: `2025-01-12-add-mcp-server-support.md`

2. **Use this template structure**:

````markdown
# [Feature/Task Name] Implementation Plan

## Overview

[Brief description of what we're implementing and why]

## Current State Analysis

[What exists now, what's missing, key constraints discovered]

### Key Discoveries:
- [Important finding with file:line reference]
- [Pattern to follow]
- [Constraint to work within]

## Desired End State

[Specification of desired end state and how to verify it]

## What We're NOT Doing

[Explicitly list out-of-scope items to prevent scope creep]

## Implementation Approach

[High-level strategy and reasoning]

## Phase 1: [Descriptive Name]

### Overview
[What this phase accomplishes]

### Changes Required:

#### 1. [Component/File Group]
**File**: `path/to/file.rs`
**Changes**: [Summary of changes]

```rust
// Specific code to add/modify
```

### Success Criteria:

#### Automated Verification:
- [ ] Builds successfully: `cargo build -p crate_name`
- [ ] Tests pass: `cargo test -p crate_name`
- [ ] Linting passes: `cargo clippy --workspace -- -D warnings`
- [ ] Formatting passes: `cargo +nightly fmt --check`

#### Manual Verification:
- [ ] Feature works as expected when tested
- [ ] No regressions in related features
- [ ] Edge cases handled correctly

**Implementation Note**: After completing automated verification, pause for manual confirmation before proceeding to next phase.

---

## Phase 2: [Descriptive Name]

[Similar structure with both automated and manual success criteria...]

---

## Testing Strategy

### Unit Tests:
- [What to test]
- [Key edge cases]

### Integration Tests:
- [End-to-end scenarios]

### Manual Testing Steps:
1. [Specific step to verify feature]
2. [Another verification step]
3. [Edge case to test manually]

## Performance Considerations

[Any performance implications or optimizations needed]

## Documentation Updates

- [ ] Update autodocs if user-facing feature
- [ ] Update AGENTS.md if development workflow changes
- [ ] Add inline documentation for public APIs

## References

- Related research: `thoughts/shared/research/[relevant].md`
- Similar implementation: `[file:line]`
- Autodocs: `autodocs/docs/[category]/[feature].md`
````

### Step 5: Review and Iterate

1. **Present the draft plan location**:
   ```
   I've created the implementation plan at:
   `thoughts/shared/plans/YYYY-MM-DD-description.md`
   
   Please review:
   - Are the phases properly scoped?
   - Are the success criteria specific enough?
   - Any technical details needing adjustment?
   - Missing edge cases or considerations?
   ```

2. **Iterate based on feedback**:
   - Add missing phases
   - Adjust technical approach
   - Clarify success criteria
   - Add/remove scope items

3. **Continue refining** until satisfied

## Important Guidelines

1. **Be Thorough**:
   - Read all context files COMPLETELY
   - Research actual code patterns
   - Include specific file:line references
   - Write measurable success criteria

2. **Be Interactive**:
   - Don't write full plan in one shot
   - Get buy-in at each major step
   - Allow course corrections
   - Work collaboratively

3. **Be Practical**:
   - Focus on incremental, testable changes
   - Consider migration and rollback
   - Think about edge cases
   - Include "what we're NOT doing"

4. **Use Cargo Commands**:
   - Reference AGENTS.md for correct commands
   - Include specific test commands
   - Note which crate to build/test
   - Use proper clippy/fmt commands

5. **No Open Questions in Final Plan**:
   - Resolve all questions before finalizing
   - Research or ask for clarification
   - Every decision must be made
   - Plan must be complete and actionable

## Success Criteria Guidelines

**Always separate into two categories:**

1. **Automated Verification** (can be run by agents):
   - `cargo build -p crate_name`
   - `cargo test -p crate_name --lib test_name`
   - `cargo clippy --workspace -- -D warnings`
   - `cargo +nightly fmt --check`

2. **Manual Verification** (requires human testing):
   - UI/UX functionality
   - Performance under real conditions
   - Edge cases hard to automate
   - User acceptance criteria

## Common Patterns

### For New Features:
- Research existing patterns first
- Start with data model/types
- Build core logic
- Add tests
- Expose via CLI/API
- Update documentation

### For Refactoring:
- Document current behavior
- Plan incremental changes
- Maintain backwards compatibility
- Include migration strategy

### For Bug Fixes:
- Reproduce the issue
- Identify root cause
- Plan minimal fix
- Add regression tests

## Important Notes

- **Always check autodocs** for existing patterns and documentation
- **Use code tool** to find similar implementations
- **Reference AGENTS.md** for correct build/test commands
- **Think in Rust patterns** - traits, modules, error handling
- **Plan for testing** - unit, integration, and manual
- **Consider documentation** - autodocs updates for user-facing features
- **Be specific** - exact file paths, line numbers, command syntax
- **Resolve all questions** before finalizing the plan

## Remember

You're creating a roadmap that enables fast, confident implementation. The plan should be:
- **Complete** - No open questions
- **Actionable** - Clear steps with verification
- **Realistic** - Achievable phases
- **Testable** - Clear success criteria

A good plan lets developers implement features quickly without getting stuck.
