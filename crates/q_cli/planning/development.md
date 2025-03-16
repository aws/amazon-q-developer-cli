# Development Guidelines

Always follow these guidelines when assisting in development for the Amazon Q CLI.

## Verifying Fixes

1. After completing a fix, verify by running build and test for the q_cli crate only. Fix any problems found.
1. Once passing, run `cargo +nightly fmt`
1. Commit changes to git. Fix any pre-commit hook errors.

## Git

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

### Push/Merging Commits

Commit all you want locally, but DO NOT push your changes to remote, ever. I will handle doing that.
