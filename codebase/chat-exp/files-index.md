# Some info about the initialization process

## Startup Call Chain
- [main()](../../crates/chat-cli/src/main.rs) - parses arguments, creates tokio runtime, passes to Cli.execute in...
- [Cli.execute()](../../crates/chat-cli/src/cli/mod.rs#L217) - sets up logger, creates `Os`, executes subcommand (below), closes telemetry
- [RootSubcommand.execute()](../../crates/chat-cli/src/cli/mod.rs#L139) - telemetry, passes to the actuall subcommand execution
  - subcommands are defined as a [enum RootSubcommand](../../crates/chat-cli/src/cli/mod.rs#L93)
  - We are intersted in `Chat(ChatArgs)`
  - `ChatArgs` are defined in "chat" folder:  [ChatArgs](../../crates/chat-cli/src/cli/chat/mod.rs#L210)
- **Chat entry point is** [`ChatArgs.execute()](../../crates/chat-cli/src/cli/chat/mod.rs#L238)
    - it makes a lot of checks and validations, creates some data
    - Mainly it kicks off [`ChatSession::new`] - [link](../../crates/chat-cli/src/cli/chat/mod.rs#L422)
    - TODO: review and list what kind of information is obtained and configured at this stage
- `ChatSession::new` - [link](../../crates/chat-cli/src/cli/chat/mod.rs#L604)


