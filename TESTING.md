# Testing Guide

This document covers testing conventions for the `chat-cli` crate, including when to use
standard assertions vs snapshot testing with [`insta`](https://insta.rs).

## Table of Contents

1. [Core Principle](#core-principle)
2. [Quick Reference](#quick-reference)
3. [Standard Assertions](#standard-assertions)
4. [Snapshot Testing with insta](#snapshot-testing-with-insta)
5. [Handling ANSI Escape Codes](#handling-ansi-escape-codes)
6. [Real Examples from This Codebase](#real-examples-from-this-codebase)
7. [Anti-Patterns to Avoid](#anti-patterns-to-avoid)

---

## Core Principle

The choice between `assert_eq!` and `insta` is not a matter of preference — it depends on
**the nature of the output being tested**:

```
Simple & deterministic output   →  assert_eq! / assert!
Complex / long string output    →  insta::assert_snapshot!
Output that may change over time →  insta (with review workflow)
```

---

## Quick Reference

| Situation | Use |
|-----------|-----|
| Numeric / boolean / enum result | `assert_eq!` / `assert!` |
| Short, stable string | `assert_eq!` |
| Formatted / multi-line string output | `insta::assert_snapshot!` |
| Complex struct | `insta::assert_debug_snapshot!` |
| JSON response | `insta::assert_json_snapshot!` |
| Output contains ANSI escape codes | Strip first → then `insta` or `assert_eq!` |
| Output that may change over time | `insta` (with review workflow) |

---

## Standard Assertions

Use `assert_eq!` and `assert!` for simple, deterministic values where the expected result
is obvious and stable.

### ✅ Correct usage

```rust
// Numeric
assert_eq!(lines_added, 3);
assert_eq!(count_sign(&output, '-'), 0);

// Boolean / Result
assert!(result.is_ok());
assert!(path.exists());

// Enum variant
assert!(matches!(fw, FsWrite::Create { .. }));

// Short stable string
assert_eq!(sanitize_path(&os, "~/file.txt"), "/home/user/file.txt");

// Condition without caring about exact value
assert!(count_sign(&output, '+') > 0);
```

### ❌ Do not use assert_eq! for these

```rust
// Long formatted string — fragile and hard to read on failure
assert_eq!(
    output,
    "- 1   : old line\n+    1: new line\n\n"
);

// Output containing ANSI escape codes — will never match visible text
assert_eq!(rendered, "\u{1b}[38;5;9m- 1   : old line\u{1b}[0m\n");

// Long JSON or structured text
assert_eq!(json_output, "{\"key\": \"value\", \"nested\": {...}}");
```

Avoid using `assert_eq!` for long or formatted strings — they are fragile and hard to read
when they fail.

---

## Snapshot Testing with insta

`insta` is already a workspace dependency. Use it when the output is a complex or formatted
string that would be tedious and brittle to hardcode manually.

### Setup

`insta` as a library is already declared in the workspace:

```toml
# Cargo.toml (workspace root)
insta = "1.43.1"

# crates/chat-cli/Cargo.toml
[dev-dependencies]
insta.workspace = true
```

Install the review CLI once before writing snapshot tests:

```bash
cargo install cargo-insta

# Verify
cargo insta --version
# cargo-insta 1.47.2
```

> **Important:** Check `Cargo.toml` for existing dependencies before adding new ones.
> `strip_ansi_escapes` is also already available in the workspace — no need to add it.

### Three modes

```rust
// 1. Inline snapshot — value stored directly in source code (recommended)
insta::assert_snapshot!(output, @"expected value here");

// 2. Auto-named — stored in a separate .snap file, named after the test function
insta::assert_snapshot!(output);

// 3. Explicitly named — stored in a separate .snap file with a custom name
insta::assert_snapshot!("my_snapshot_name", output);
```

**Recommendation for this codebase:** use **inline snapshots** (`@"..."`) for short-to-medium
output. The expected value is visible in the same file as the test, making PR reviews easier
without needing to open separate `.snap` files.

### Inline snapshots (recommended)

```rust
insta::assert_snapshot!(output, @"
- 1   : old line
+    1: new line
");
```

### Named / auto-named snapshots

For larger output, let `insta` store the snapshot in a separate `.snap` file:

```rust
// Auto-named (uses test function name)
insta::assert_snapshot!(output);

// Explicitly named
insta::assert_snapshot!("my_snapshot", output);
```

Snapshot files are committed to the repository alongside the test.

### Other snapshot variants

```rust
// Debug output of any type that implements Debug
insta::assert_debug_snapshot!(parsed_tool_use);

// JSON (requires the "json" feature, already enabled in workspace)
insta::assert_json_snapshot!(api_response);
```

### Workflow

**Step 1:** Write the test without an expected value:

```rust
#[test]
fn test_diff_output() {
    let output = diff_output("old line\n", "new line\n");
    insta::assert_snapshot!(output);  // no value yet
}
```

**Step 2:** Run the tests — they will fail but record the actual output as pending:

```bash
cargo test -p chat_cli test_diff_output
# FAILED — but snapshot is saved as pending
```

**Step 3:** Review the recorded output interactively:

```bash
cargo insta review
```

This opens an interactive prompt:

```
Snapshot: diff_output
Source: crates/chat-cli/src/cli/chat/tools/fs_write.rs:1615
────────────────────────────────────────────────────────
Expression: output
────────────────────────────────────────────────────────
New snapshot:
- 1   : old line
+    1: new line

Accept? [y/n/s(kip)]
```

**Step 4:** Accept the snapshot:

```bash
# Press 'y' in the interactive review, or accept all at once:
cargo insta accept
```

After accepting, the source file is updated automatically with the inline value:

```rust
insta::assert_snapshot!(output, @"
- 1   : old line
+    1: new line
");
```

**Step 5:** Re-run the tests — they should now pass:

```bash
cargo test -p chat_cli test_diff_output
# ok
```

**When output changes in the future:**

```bash
cargo insta test -p chat_cli  # run all tests with insta
cargo insta review             # review changes: regression or intentional?
```

---

## Handling ANSI Escape Codes

Terminal rendering functions (e.g. `print_diff`) embed ANSI color codes in their output.
Asserting directly on that output will fail because lines start with escape sequences, not
the visible characters:

```
\u{1b}[38;5;9m- 1   : old line\u{1b}[0m
```

The line above starts with `\u{1b}[38;5;9m` (a red color code), not `-`. Any assertion
checking for `starts_with('-')` will silently fail.

Strip ANSI codes before asserting. The `strip_ansi_escapes` crate is already used in the
codebase — no need to add a new dependency:

```rust
fn diff_output(old: &str, new: &str) -> String {
    let old = StylizedFile { content: old.to_string(), ..Default::default() };
    let new = StylizedFile { content: new.to_string(), ..Default::default() };
    let mut buf = Vec::new();
    print_diff(&mut buf, &old, &new, 1).unwrap();
    // strip_ansi_escapes is already in the workspace — check Cargo.toml before adding deps
    strip_ansi_escapes::strip_str(String::from_utf8(buf).unwrap())
}
```

---

## Real Examples from This Codebase

### Case 1: Testing `print_diff` in `fs_write.rs`

`print_diff` is a private function that renders a colored terminal diff. It embeds ANSI
codes and produces multi-line formatted output — a perfect case for combining both approaches.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Renders print_diff to a plain string with ANSI codes stripped.
    fn diff_output(old: &str, new: &str) -> String {
        let old = StylizedFile { content: old.to_string(), ..Default::default() };
        let new = StylizedFile { content: new.to_string(), ..Default::default() };
        let mut buf = Vec::new();
        print_diff(&mut buf, &old, &new, 1).unwrap();
        strip_ansi_escapes::strip_str(String::from_utf8(buf).unwrap())
    }

    /// Counts lines starting with the given sign character.
    fn count_sign(output: &str, sign: char) -> usize {
        output.lines().filter(|l| l.starts_with(sign)).count()
    }

    #[test]
    fn test_trailing_newline_only_change_is_not_shown() {
        // assert_eq! is sufficient — result is a simple number
        let output = diff_output("fi\n", "fi");
        assert_eq!(count_sign(&output, '-'), 0);
        assert_eq!(count_sign(&output, '+'), 0);
    }

    #[test]
    fn test_real_change_is_shown() {
        // insta is appropriate — output is a formatted string with line numbers
        let output = diff_output("old line\n", "new line\n");
        insta::assert_snapshot!(output, @"
        - 1   : old line
        +    1: new line
        ");
    }

    #[test]
    fn test_multiple_trailing_newlines_difference_is_shown() {
        // assert! is sufficient — we only need to know insertions exist
        let output = diff_output("fi\n", "fi\n\n\n");
        assert!(count_sign(&output, '+') > 0);
    }
}
```

**Pattern used:**
- `assert_eq!` for numeric results from `count_sign`
- `insta::assert_snapshot!` for the exact formatted string output
- `assert!` for simple boolean conditions

### Case 2: Testing `sanitize_path_tool_arg` in `tools/mod.rs`

```rust
#[tokio::test]
async fn test_tilde_path_expansion() {
    let os = Os::new().await.unwrap();

    // assert_eq! is appropriate — output is a deterministic path derived from os.env.home()
    let actual = sanitize_path_tool_arg(&os, "~");
    assert_eq!(actual, os.fs.chroot_path(&expected_home), "tilde should expand");

    let actual = sanitize_path_tool_arg(&os, "~/hello");
    assert_eq!(actual, os.fs.chroot_path(&expected_home.join("hello")));
}
```

`assert_eq!` is correct here because the output is a path that can be computed
deterministically from `os.env.home()` — no formatting, no ANSI, no variability.

---

## Anti-Patterns to Avoid

### ❌ Hardcoding ANSI escape codes

```rust
// Don't
assert_eq!(output, "\u{1b}[38;5;9m- 1   : old\u{1b}[0m\n");

// Do
let clean = strip_ansi_escapes::strip_str(&output);
insta::assert_snapshot!(clean, @"- 1   : old");
```

### ❌ Using insta for simple values

```rust
// Overkill
insta::assert_snapshot!(count.to_string(), @"3");

// Sufficient
assert_eq!(count, 3);
```

### ❌ Manually writing snapshot values

```rust
// Don't guess — you will get whitespace or escaping wrong
insta::assert_snapshot!(output, @"- 1   : old line\n+    1: new line\n\n");

// Let insta record the actual output, then review and accept
insta::assert_snapshot!(output);  // run first, review, then accept
```

### ❌ Not installing cargo-insta before writing snapshot tests

```bash
# Check for insta in the workspace before starting
grep insta Cargo.toml
# If present → install the CLI tool
cargo install cargo-insta
```

Skipping this leads to workarounds like hardcoding strings or modifying test helpers
unnecessarily — both of which were avoidable mistakes documented here for reference.
