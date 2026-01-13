---
description: Validate implementation against plan, verify success criteria, identify issues
---

# Validate Plan

You are tasked with validating that an implementation plan was correctly executed, verifying all success criteria and identifying any deviations or issues.

## Initial Setup

When invoked:
1. **Determine context** - Are you in an existing conversation or starting fresh?
   - If existing: Review what was implemented in this session
   - If fresh: Need to discover what was done through git and codebase analysis

2. **Locate the plan**:
   - If plan path provided, use it
   - Otherwise, search recent commits for plan references or ask user

3. **Gather implementation evidence**:
   ```bash
   # Check recent commits
   git log --oneline -n 20
   git diff HEAD~N..HEAD  # Where N covers implementation commits
   ```

## Validation Process

### Step 1: Context Discovery

If starting fresh or need more context:

1. **Read the implementation plan** completely

2. **Identify what should have changed**:
   - List all files that should be modified
   - Note all success criteria (automated and manual)
   - Identify key functionality to verify

3. **Spawn parallel subagents** to discover implementation:
   ```
   Task 1 - Verify code changes:
   Find all modified files related to [feature].
   Compare actual changes to plan specifications.
   Return: File-by-file comparison of planned vs actual
   
   Task 2 - Verify test coverage:
   Check if tests were added/modified as specified.
   Run test commands and capture results.
   Return: Test status and any missing coverage
   ```

### Step 2: Systematic Validation

For each phase in the plan:

1. **Check completion status**:
   - Look for checkmarks in the plan (- [x])
   - Verify the actual code matches claimed completion

2. **Run automated verification**:
   ```bash
   # Build
   cargo build -p crate_name
   
   # Run tests
   cargo test -p crate_name
   
   # Linting
   cargo clippy --locked --workspace --color always -- -D warnings
   
   # Formatting
   cargo +nightly fmt --check -- --color always
   ```
   
   See AGENTS.md for more commands.

3. **Document pass/fail status**:
   - If failures, investigate root cause
   - Check logs if needed (see AGENTS.md for log locations)

4. **Assess manual criteria**:
   - List what needs manual testing
   - Provide clear steps for user verification

5. **Think deeply about edge cases**:
   - Were error conditions handled?
   - Are there missing validations?
   - Could the implementation break existing functionality?

### Step 3: Generate Validation Report

Create comprehensive validation summary:

```markdown
## Validation Report: [Plan Name]

### Implementation Status
✓ Phase 1: [Name] - Fully implemented
✓ Phase 2: [Name] - Fully implemented
⚠️ Phase 3: [Name] - Partially implemented (see issues)

### Automated Verification Results

#### Build Status
✓ `cargo build -p crate_name` - Success

#### Test Results
✓ `cargo test -p crate_name` - All tests passing
- X unit tests passed
- Y integration tests passed

#### Linting
✓ `cargo clippy --workspace -- -D warnings` - No warnings

#### Formatting
✓ `cargo +nightly fmt --check` - All files formatted correctly

### Code Review Findings

#### Matches Plan:
- API endpoints implement specified methods
- Error handling follows plan
- Tests cover specified scenarios

#### Deviations from Plan:
- Used different variable names in [file:line] (minor)
- Added extra validation in [file:line] (improvement)

#### Potential Issues:
- Missing edge case handling for [scenario]
- Performance consideration for [operation]

### Manual Testing Required:

1. Feature functionality:
   - [ ] Verify [feature] works correctly
   - [ ] Test error states with invalid input
   - [ ] Check edge cases: [list specific cases]

2. Integration:
   - [ ] Confirm works with existing [component]
   - [ ] Check performance with realistic data

3. User experience:
   - [ ] CLI output is clear and helpful
   - [ ] Error messages are actionable

### Documentation Status
- [ ] Autodocs updated (if user-facing feature)
- [ ] Inline documentation added for public APIs
- [ ] AGENTS.md updated (if workflow changes)

### Recommendations:
- Address [specific issue] before merge
- Consider adding [test scenario]
- Update [documentation]
```

## Working with Existing Context

If you were part of the implementation:
- Review the conversation history
- Check your todo list for what was completed
- Focus validation on work done in this session
- Be honest about any shortcuts or incomplete items

## Using Code Tool for Verification

Use `code` tool to verify implementation:
```bash
# Find all implementations of a trait
code pattern_search --pattern "impl $TRAIT for $TYPE" --language rust

# Verify function signatures
code lookup_symbols --symbols '["function_name"]' --include-source true

# Check file structure
code get_document_symbols --file-path "src/file.rs"
```

## Important Guidelines

1. **Be thorough but practical** - Focus on what matters
2. **Run all automated checks** - Don't skip verification commands
3. **Document everything** - Both successes and issues
4. **Think critically** - Question if the implementation truly solves the problem
5. **Consider maintenance** - Will this be maintainable long-term?
6. **Use subagents** - Delegate verbose build/test output to keep context clean

## Validation Checklist

Always verify:
- [ ] All phases marked complete are actually done
- [ ] Automated tests pass
- [ ] Code follows existing patterns
- [ ] No regressions introduced
- [ ] Error handling is robust
- [ ] Documentation updated if needed
- [ ] Manual test steps are clear
- [ ] Linting passes
- [ ] Formatting is correct

## Relationship to Other Commands

Recommended workflow:
1. `@implement` - Execute the implementation
2. `@validate` - Verify implementation correctness (you are here)
3. `@commit` - Create atomic commits for changes

The validation works best after implementation is complete, as it can analyze the changes comprehensively.

## Common Issues to Check

### Rust-Specific:
- Proper error handling (Result types)
- Ownership and borrowing correctness
- Trait implementations complete
- Public API documentation
- Test coverage for error cases

### General:
- Edge cases handled
- Performance considerations
- Breaking changes documented
- Backwards compatibility maintained

## Important Notes

- **Run all automated checks** - build, test, clippy, fmt
- **Use AGENTS.md** for correct command syntax
- **Check logs** if tests fail (see AGENTS.md for locations)
- **Use code tool** to verify structure and patterns
- **Think about edge cases** - what could go wrong?
- **Document deviations** - note any changes from plan
- **Be constructive** - identify issues with solutions
- **Consider maintenance** - is this code sustainable?
- **Verify documentation** - are user-facing changes documented?

## Remember

Good validation catches issues before they reach production. Be constructive but thorough in identifying gaps or improvements.

Focus on:
- **Correctness** - Does it work as intended?
- **Completeness** - Are all requirements met?
- **Quality** - Does it follow best practices?
- **Maintainability** - Can others understand and modify it?

Your validation should give confidence that the implementation is production-ready.
