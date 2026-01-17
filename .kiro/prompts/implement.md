---
description: Implement technical plans from thoughts/shared/plans with verification
---

# Implement Plan

You are tasked with implementing an approved technical plan from `thoughts/shared/plans/`. These plans contain phases with specific changes and success criteria. Your goal is to implement features quickly and correctly.

## Getting Started

When given a plan path:
- Read the plan completely and check for existing checkmarks (- [x])
- Read all files mentioned in the plan FULLY
- **Read files completely** - never use limit/offset, you need full context
- Think deeply about how pieces fit together
- **Create a todo list** to track progress through the implementation
- Start implementing if you understand what needs to be done

If no plan path provided, ask for one.

## Using Todo Lists

**ALWAYS create a todo list** when starting implementation:

```
todo_list create with tasks:
- Phase 1: [Phase name from plan]
- Phase 2: [Phase name from plan]
- Phase 3: [Phase name from plan]
```

**Update the todo list** as you progress:
- Mark tasks complete after finishing each phase
- Add context about what was accomplished
- Track modified files
- Note any deviations from the plan

**Benefits of todo lists**:
- Keeps you organized across long implementations
- Provides clear progress tracking
- Helps resume work if interrupted
- Documents what was done in each phase

Example workflow:
```
1. Create todo list from plan phases
2. Implement Phase 1
3. Run automated verification
4. Mark Phase 1 complete with context
5. Pause for manual verification
6. Continue to Phase 2
```

## Implementation Philosophy

Plans are carefully designed, but reality can be messy. Your job is to:
- Follow the plan's intent while adapting to what you find
- Implement each phase fully before moving to next
- Verify your work makes sense in broader codebase context
- Update checkboxes in the plan as you complete sections
- Update your todo list to track progress

When things don't match the plan exactly:
- STOP and think deeply about why
- Present the issue clearly:
  ```
  Issue in Phase [N]:
  Expected: [what the plan says]
  Found: [actual situation]
  Why this matters: [explanation]
  
  How should I proceed?
  ```

## Approach

Use subagents for build/test verification when output is verbose:
- Delegate to subagents to avoid polluting main context
- You MUST run full build to confirm functionality
- This includes compilation, linting, and testing

## Verification Approach

After implementing a phase:

1. **Run automated verification strategically**:
   
   **When to run cargo commands:**
   - After completing a substantial phase with multiple file changes
   - After implementing core logic that affects compilation
   - Before pausing for manual verification
   - NOT after every small change (adding a single method, fixing formatting, etc.)
   
   **Verification commands** (run together after substantial work):
   ```bash
   # Format first
   cargo +nightly fmt
   
   # Then build and test
   cargo build -p crate_name
   cargo test -p crate_name
   
   # Finally lint (most comprehensive, run last)
   cargo clippy --locked --workspace --color always -- -D warnings
   ```
   
   **Examples of when to verify:**
   - ✅ After implementing entire Settings struct changes (Phase 2)
   - ✅ After updating all CLI commands (Phase 3)
   - ✅ After adding comprehensive tests (Phase 4)
   - ❌ After adding a single method to a struct
   - ❌ After fixing a typo or formatting issue
   - ❌ After each small file edit
   
   See AGENTS.md for more commands and examples.

2. **Fix any issues** before proceeding

3. **Update progress**:
   - Check off completed items in the plan file using fs_write
   - Update your todo list with context and modified files
   - Document any deviations or important decisions

4. **Run comprehensive tests before documentation**:
   Before moving to documentation updates or manual verification, run:
   ```bash
   cargo test --locked --workspace --lib --bins --test '*'
   ```
   This ensures all unit and integration tests pass across the entire workspace.

5. **Pause for human verification**:
   After completing all automated verification for a phase:
   ```
   Phase [N] Complete - Ready for Manual Verification
   
   Automated verification passed:
   - [List automated checks that passed]
   
   Please perform manual verification steps from the plan:
   - [List manual verification items]
   
   Let me know when manual testing is complete so I can proceed to Phase [N+1].
   ```

If instructed to execute multiple phases consecutively, skip the pause until the last phase.

Do not check off manual testing items until confirmed by the user.

## If You Get Stuck

When something isn't working:
- Make sure you've read and understood all relevant code
- Consider if codebase has evolved since plan was written
- Present the mismatch clearly and ask for guidance

Use subagents sparingly - mainly for targeted debugging or exploring unfamiliar territory.

## Resuming Work

If the plan has existing checkmarks:
- Trust that completed work is done
- Pick up from first unchecked item
- Verify previous work only if something seems off

If you have an existing todo list:
- Load it with `todo_list load`
- Review completed tasks and context
- Continue from the next uncompleted task

## Development Workflow

### Building and Testing

Reference AGENTS.md for detailed commands:

```bash
# Quick build check
cargo build -p crate_name

# Run all tests in a crate
cargo test -p crate_name

# Run specific test
cargo test -p crate_name --lib test_name

# Run single test with full path
cargo test -p chat_cli --bin chat_cli cli::chat::cli::persist::tests::test_save_and_load_file

# Run all tests in a module
cargo test -p chat_cli --bin chat_cli persist::tests

# Linting
cargo clippy --locked --workspace --color always -- -D warnings

# Formatting
cargo +nightly fmt
cargo +nightly fmt --check -- --color always
```

### Log Files

Check logs when debugging:
- **macOS/Linux**: `$TMPDIR/kiro-log/kiro-chat.log`
- **Windows**: `%TEMP%/kiro-log/logs/kiro-chat.log`
- **MCP logs**: Same directory, `mcp.log`

### Using Code Tool

When implementing, use `code` tool to:
- Find similar implementations: `code search_symbols --symbol-name "Pattern"`
- Understand file structure: `code get_document_symbols --file-path "src/file.rs"`
- Look up function signatures: `code lookup_symbols --symbols '["function_name"]'`

### Common Patterns

1. **Adding a new feature**:
   - Define types/structs
   - Implement core logic
   - Add tests
   - Wire up to CLI/API
   - Update documentation

2. **Modifying existing feature**:
   - Find all usage with `code` tool
   - Update implementation
   - Update tests
   - Verify no regressions

3. **Fixing a bug**:
   - Add failing test first
   - Fix the issue
   - Verify test passes
   - Check for similar issues

## Testing Standards

When adding tests:
- Follow Rust testing conventions
- Use `#[test]` for unit tests
- Use `tests/` directory for integration tests
- Test both success and error cases
- Use descriptive test names

Example:
```rust
#[test]
fn test_feature_handles_valid_input() {
    // Arrange
    let input = "valid";
    
    // Act
    let result = feature_function(input);
    
    // Assert
    assert!(result.is_ok());
}

#[test]
fn test_feature_rejects_invalid_input() {
    let input = "invalid";
    let result = feature_function(input);
    assert!(result.is_err());
}
```

## Documentation Updates

If implementing user-facing features:
- Check if autodocs need updates
- Update relevant docs in `autodocs/docs/`
- Follow autodocs README.md guidelines
- Run quality checks if updating docs

## Important Notes

- **Always create a todo list** at the start of implementation
- **Update todo list** after completing each phase
- **Follow the plan** but adapt when reality differs
- **Verify thoroughly** - run all automated checks
- **Update checkboxes** in the plan as you progress
- **Pause for manual verification** between phases
- **Use AGENTS.md** for correct cargo commands
- **Use code tool** to find patterns and understand structure
- **Test comprehensively** - unit and integration tests
- **Check logs** when debugging issues
- **Update documentation** for user-facing changes
- **Think in Rust** - ownership, borrowing, error handling
- **Keep context clean** - use subagents for verbose operations

## Remember

You're implementing a solution, not just checking boxes. Keep the end goal in mind and maintain forward momentum. The plan is your guide, but your judgment matters too.

Focus on:
- **Correctness** - Does it work as intended?
- **Quality** - Does it follow Rust best practices?
- **Completeness** - Are all edge cases handled?
- **Testability** - Can it be verified?

Your implementation should be production-ready and maintainable.
