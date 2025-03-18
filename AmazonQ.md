# Amazon Q Development Guidelines

Always follow these guidelines when assisting in development for the Amazon Q CLI.

## AmazonQ.md

DO NOT create or modify an AmazonQ.md file unless I explicitly tell you to do so.

## Rust Best Practices

### File Operations

When working with file operations in Rust:

1. Prefer using the simpler `fs::read_to_string()` and `fs::write()` functions over verbose `File::open()` + `read_to_string()` or `File::create()` + `write_all()` combinations
2. Avoid the `#[allow(clippy::verbose_file_reads)]` annotation by using the recommended methods
3. Use `serde_json::to_string_pretty()` + `fs::write()` instead of creating a file and then writing to it with `serde_json::to_writer_pretty()`
4. Keep imports organized by functionality (e.g., group path-related imports together)

## Git

### Committing Changes

Follow the git best practice of committing early and often. Run `git commit` often, but DO NOT ever run `git push`

BEFORE committing a change, ALWAYS do the following steps:

1. Run `cargo build` and fix any problems. Prefer running it against just the crate you're modifying for shorter runtimes
2. Run `cargo test` and fix any problems. Prefer running it against just the crate you're modifying for shorter runtimes
3. Run `cargo +nightly fmt` to auto-format the code
4. Commit the changes

### Commit Messages

All commit messages should follow the [Conventional Commits](https://www.conventionalcommits.org/) specification and include best practices:

```
<type>[optional scope]: <description>

[optional body]

[optional footer(s)]

🤖 Assisted by [Amazon Q Developer](https://aws.amazon.com/q/developer)
```

Types:
- feat: A new feature
- fix: A bug fix
- docs: Documentation only changes
- style: Changes that do not affect the meaning of the code
- refactor: A code change that neither fixes a bug nor adds a feature
- perf: A code change that improves performance
- test: Adding missing tests or correcting existing tests
- chore: Changes to the build process or auxiliary tools
- ci: Changes to CI configuration files and scripts

Best practices:
- Use the imperative mood ("add" not "added" or "adds")
- Don't end the subject line with a period
- Limit the subject line to 50 characters
- Capitalize the subject line
- Separate subject from body with a blank line
- Use the body to explain what and why vs. how
- Wrap the body at 72 characters

Example:
```
feat(lambda): Add Go implementation of DDB stream forwarder

Replace Node.js Lambda function with Go implementation to reduce cold
start times. The new implementation supports forwarding to multiple SQS
queues and maintains the same functionality as the original.

🤖 Assisted by [Amazon Q Developer](https://aws.amazon.com/q/developer)
```
