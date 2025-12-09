---
description: Fix all Rust clippy errors and format code
---

Run "cargo clippy --locked --color always -- -D warnings" to identify all compilation errors

Analyze the compiler output and fix every issue

Continue this process until the project compiles successfully.

Then run "cargo +nightly fmt" to format the code
