// lib.rs
// Main module for Amazon Q CLI automatic naming feature

pub mod conversation;
pub mod filename_generator;
pub mod topic_extractor;
pub mod save_config;
pub mod commands;
pub mod security;
pub mod integration_checkpoint_1;
pub mod integration_checkpoint_2;
pub mod integration_checkpoint_3;

#[cfg(test)]
pub mod tests;

// Re-export main components
pub use conversation::Conversation;
pub use filename_generator::generate_filename;
pub use topic_extractor::extract_topics;
pub use save_config::SaveConfig;
pub use commands::CommandRegistry;
pub use security::SecuritySettings;
pub use integration_checkpoint_1::{test_integration as test_integration_1, example_usage as example_usage_1, document_issues as document_issues_1};
pub use integration_checkpoint_2::{test_integration as test_integration_2, example_usage as example_usage_2, document_issues as document_issues_2};
pub use integration_checkpoint_3::{test_integration as test_integration_3, example_usage as example_usage_3, document_issues as document_issues_3};
