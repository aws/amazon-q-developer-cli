# Testing Guide

This document covers testing conventions for the `chat-cli` crate, including when to use
standard assertions vs snapshot testing with [`insta`](https://insta.rs).

## Quick Reference

| Situation | Use |
|-----------|-----|
| Numeric / boolean / enum result | `assert_eq!` / `assert!` |
| Short, stable string | `assert_eq!` |
| Formatted / multi-line string output | `insta::assert_snapshot!` |
| Complex struct | `insta::assert_debug_snapshot!` |
| Output contains ANSI escape codes | Strip first → then `insta` or `assert_eq!` |

---

## Standard Assertions

Use `assert_eq!` and `assert!` for simple, deterministic values where the expected result
is obvious and stable:

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
```

Avoid using `assert_eq!` for long or formatted strings — they are fragile and hard to read
when they fail.

---

## Snapshot Testing with insta

`insta` is already a workspace dependency. Use it when the output is a complex or formatted
string that would be tedious and brittle to hardcode manually.

### Setup

Install the review CLI once:

```bash
cargo install cargo-insta
```

### Inline snapshots (recommended)

For short-to-medium output, store the expected value directly in source using `@"..."`:

```rust
insta::assert_snapshot!(output, @"
- 1   : old line
+    1: new line
");
```

This keeps the expected value visible in the same file as the test, making PR reviews easier.

### Named / auto-named snapshots

For larger output, let `insta` store the snapshot in a separate `.snap` file:

```rust
// Auto-named (uses test function name)
insta::assert_snapshot!(output);

// Explicitly named
insta::assert_snapshot!("my_snapshot", output);
```

Snapshot files are committed to the repository alongside the test.

### Workflow

1. Write the test without an expected value:

   ```rust
   insta::assert_snapshot!(output);
   ```

2. Run the tests — they will fail but record the actual output:

   ```bash
   cargo test -p chat_cli
   ```

3. Review and accept the recorded output:

   ```bash
   cargo insta review   # interactive: inspect each snapshot
   # or
   cargo insta accept   # accept all pending snapshots at once
   ```

4. Re-run the tests — they should now pass:

   ```bash
   cargo test -p chat_cli
   ```

5. When output changes in the future, `cargo insta review` shows a before/after diff.
   Decide whether the change is intentional or a regression.

### Other snapshot variants

```rust
// Debug output of any type that implements Debug
insta::assert_debug_snapshot!(parsed_tool_use);

// JSON (requires the "json" feature, already enabled in workspace)
insta::assert_json_snapshot!(api_response);
```

---

## Handling ANSI Escape Codes

Terminal rendering functions (e.g. `print_diff`) embed ANSI color codes in their output.
Asserting directly on that output will fail because lines start with escape sequences, not
the visible characters.

Strip ANSI codes before asserting. The `strip_ansi_escapes` crate is already used in the
codebase:

```rust
// In your test helper
fn render(old: &str, new: &str) -> String {
    let mut buf = Vec::new();
    print_diff(&mut buf, &make_file(old), &make_file(new), 1).unwrap();
    strip_ansi_escapes::strip_str(String::from_utf8(buf).unwrap())
}

#[test]
fn test_real_change_is_shown() {
    let output = render("old line\n", "new line\n");
    insta::assert_snapshot!(output, @"
    - 1   : old line
    +    1: new line
    ");
}
```

---

## Real Example: `print_diff` in `fs_write.rs`

The tests added in [#3717](https://github.com/aws/amazon-q-developer-cli/pull/3717) demonstrate
this pattern in practice:

- `count_sign()` + `assert_eq!` for simple numeric assertions (zero deletions, non-zero insertions)
- `insta::assert_snapshot!` for the exact formatted output of a real change
- `strip_ansi_escapes` to make the output assertable

See `crates/chat-cli/src/cli/chat/tools/fs_write.rs` tests module for the full implementation.
