// Main integration test file that includes all subdirectory tests
mod agent;
mod ai_prompts;
mod context;
mod core_session;
mod integration;
mod mcp;
mod model;
mod q_subcommand;
mod save_load;
mod session_mgmt;
mod tools;
mod todos;
mod experiment;

use q_cli_e2e_tests::q_chat_helper;
use std::time::Instant;

#[ctor::dtor]
fn cleanup_session() {
    let _ = q_chat_helper::close_session();
}