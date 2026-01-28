---
description: Create git commits with user approval and no Kiro attribution
---

# Commit Changes

You are tasked with creating git commits for the changes made during this session.

## CRITICAL: Follow Steps in Order

**DO NOT SKIP STEP 1. ALWAYS ASK ABOUT BRANCH PREFERENCE FIRST.**

## Process

### 1. Check branch preference (REQUIRED - DO NOT SKIP)

**STOP. Before doing anything else, ask the user:**

```
Would you like to:
1. Create a new branch from main for these changes
2. Use the current branch

Please choose 1 or 2.
```

**Wait for user response before proceeding.**

If option 1:
- Ask for branch name
- Run `git checkout main && git pull && git checkout -b <branch-name>`

If option 2:
- Continue with current branch

### 2. Think about what changed

- Review the conversation history and understand what was accomplished
- Run `git status` to see current changes
- Run `git diff` to understand the modifications

### 3. Plan your commit(s)

- Identify which files belong together
- Draft clear, descriptive commit messages
- Use imperative mood in commit messages
- Focus on why the changes were made
- Commit message format:
  ```
  type: Short description (max 72 chars)
  
  Problem: Paragraph explaining problem being solved
  
  Solution: Summary explaining solution, including list of
  key changes made to the project
  
  Key changes:
  - bullet points explaining key project changes
  - ...
  - ...
  
  Testing: high-level summary of testing changes/improvements.
  Do not go into too much details, just high-level changes
  ```

**Commit types:**
- `feat`: New feature
- `fix`: Bug fix
- `chore`: Maintenance, refactoring, dependencies
- `docs`: Documentation only
- `test`: Test additions or modifications
- `perf`: Performance improvements

**Example:**
```
feat: Add MCP server configuration support

Problem: Users needed a way to configure MCP servers through
the CLI without manually editing configuration files.

Solution: Added new CLI commands for managing MCP server
configurations, including add, remove, and list operations.

Key changes:
- Added mcp_config module with server management functions
- Implemented CLI commands in cli/mcp.rs
- Added configuration persistence to settings
- Added comprehensive tests for MCP operations

Testing: Added unit tests for configuration management and
integration tests for CLI commands. All tests passing.
```

### 4. Present your plan to the user

- List the files you plan to add for each commit
- Show the commit message(s) you'll use
- Ask: "I plan to create a commit with these changes. Shall I proceed?"

### 5. Execute upon confirmation

- Use `git add` with specific files (never use `-A` or `.`)
- Create commits with your planned messages
- Show the result with `git log --oneline -n [number]`

### 6. Create changelog fragment (if needed)

Before committing, check if a changelog fragment is needed:

**Create fragment when ALL of these are true:**
- Changes are in the `crates/` folder (not .github/, .kiro/, docs/, etc.)
- Changes modify user-visible behavior (CLI output, commands, features)
- Changes are NOT internal refactors, dependency updates, or code cleanup

**When in doubt:** Ask "Will users notice this change?" If no, skip the fragment.

If a fragment is needed, ask the user:
```
This PR includes user-facing changes. Would you like me to create a changelog fragment?
```

If yes:
```bash
./scripts/new-change.sh <type> "<description>"
```

Types: `added`, `changed`, `deprecated`, `removed`, `fixed`, `security`

Include the fragment in your commit by adding `.changes/unreleased/*.json` to the staged files.

### 7. Ask about pushing

After creating commits, ask:
```
Would you like me to push these changes to the remote repository?
```

If yes:
- Run `git push` (or `git push -u origin <branch-name>` for new branches)
- Show the result

## Important

- **NEVER add co-author information or Kiro attribution**
- Commits should be authored solely by the user
- Do not include any "Generated with X" messages
- Do not add "Co-Authored-By" lines
- Write commit messages as if the user wrote them

## Commit Message Guidelines

### Subject Line (First Line)
- Max 72 characters
- Use imperative mood: "Add feature" not "Added feature"
- No period at the end
- Start with type: `feat:`, `fix:`, `chore:`, etc.

### Body
- Wrap at 72 columns
- Explain WHAT and WHY, not HOW
- Use bullet points for key changes
- Keep testing summary high-level

### Good Examples

```
feat: Implement agent configuration system

Problem: Users needed a way to define custom agents with
specific tools and models for different workflows.

Solution: Created agent configuration system that allows
defining agents in JSON files with tool restrictions and
model selection.

Key changes:
- Added AgentConfig struct and parsing logic
- Implemented agent loading from config directory
- Added CLI commands for agent management
- Created validation for agent configurations

Testing: Added unit tests for config parsing and validation.
Integration tests verify agent loading and CLI commands.
```

```
fix: Handle missing config file gracefully

Problem: Application crashed when config file was missing
instead of creating default configuration.

Solution: Added config file existence check and automatic
creation of default config when missing.

Key changes:
- Added config file existence check in load_config
- Implemented default config creation
- Added error handling for file system operations

Testing: Added test for missing config scenario. Verified
default config creation works correctly.
```

```
chore: Update dependencies to latest versions

Problem: Several dependencies had security updates and bug
fixes that needed to be incorporated.

Solution: Updated all dependencies to latest compatible
versions and verified no breaking changes.

Key changes:
- Updated tokio to 1.35.0
- Updated serde to 1.0.195
- Updated clap to 4.4.18

Testing: Ran full test suite to verify no regressions.
All tests passing.
```

(Note: No Changelog section needed for internal dependency updates)

```
chore: Add commit message validation to CI

Problem: Inconsistent commit message formats made it
difficult to generate automated release notes and maintain
clear project history.

Solution: Added pre-commit hook and CI check to validate
commit message format against project standards.

Key changes:
- Added commit-msg git hook script
- Implemented CI workflow for commit validation
- Added documentation for commit message format

Testing: Tested hook with valid and invalid commit messages.
CI workflow validates all commits in pull requests.
```

(Note: No Changelog section needed for CI/CD and development tooling)

## Tips for Good Commits

1. **Atomic commits**: Each commit should be a single logical change
2. **Complete commits**: Each commit should leave the codebase in a working state
3. **Descriptive commits**: Someone should understand the change without reading the code
4. **Focused commits**: Don't mix unrelated changes in one commit

## Multiple Commits

If changes span multiple features or fixes, create separate commits:

```
Commit 1: feat: Add new feature X
Commit 2: test: Add tests for feature X
Commit 3: docs: Update documentation for feature X
```

Or group logically:

```
Commit 1: feat: Add feature X with tests and docs
Commit 2: fix: Resolve edge case in feature Y
```

## Remember

- You have the full context of what was done in this session
- Group related changes together
- Keep commits focused and atomic when possible
- The user trusts your judgment - they asked you to commit
- Write messages that will be helpful in 6 months
- No Kiro attribution - commits are authored by the user
- Always ask if the user wants to push after creating commits

## Example Workflow

```
User: @commit

You: I'll review the changes and create appropriate commits.

[Run git status and git diff]

I see changes to:
- src/config.rs (new configuration loading)
- src/cli/settings.rs (new CLI commands)
- tests/config_tests.rs (new tests)

I plan to create one commit:

Type: feat
Message: Add configuration management system

Files to include:
- src/config.rs
- src/cli/settings.rs
- tests/config_tests.rs

Shall I proceed?

User: Yes

You: [Execute git add and git commit]

Created commit:
abc1234 feat: Add configuration management system

[Show git log output]

Would you like me to push these changes to the remote repository?
```
