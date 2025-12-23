## Testing

- Run single test: `cargo test -p chat_cli --bin chat_cli cli::chat::cli::persist::tests::test_save_and_load_file` or - `cargo test -p agent --lib test_mcp_server_config_stdio_deser`
- Run all tests in a module: `cargo test -p chat_cli --bin chat_cli persist::tests` (all tests in a module)

