pub mod clear;
pub mod compact;
pub mod context;
pub mod context_adapter;
pub mod editor;
pub mod execute;
pub mod handler;
pub mod help;
pub mod issue;
pub mod profile;
pub mod prompts;
pub mod quit;
pub mod test_utils;
pub mod tools;
pub mod usage;

pub use context_adapter::CommandContextAdapter;
// Keep CommandHandler as crate-only visibility
pub(crate) use handler::CommandHandler;
