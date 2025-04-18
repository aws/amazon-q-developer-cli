mod clear;
mod compact;
pub mod context;
pub mod handler;
pub mod help;
pub mod profile;
mod quit;
pub mod registry;
#[cfg(test)]
pub mod test_utils;
// We'll use the directory versions of these modules
// mod tools;

pub use clear::ClearCommand;
pub use compact::CompactCommand;
pub use context::ContextCommand;
pub use handler::CommandHandler;
pub use help::HelpCommand;
pub use profile::ProfileCommand;
pub use quit::QuitCommand;
pub use registry::CommandRegistry;
// We'll need to update these imports once we fix the module structure
// pub use tools::ToolsCommand;
