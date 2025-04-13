mod clear;
mod context;
pub mod handler;
pub mod help;
mod quit;
pub mod registry;
#[cfg(test)]
pub mod test_utils;
// We'll use the directory versions of these modules
// mod profile;
// mod tools;

pub use clear::ClearCommand;
pub use context::ContextCommand;
pub use handler::CommandHandler;
pub use help::HelpCommand;
pub use quit::QuitCommand;
pub use registry::CommandRegistry;
// We'll need to update these imports once we fix the module structure
// pub use profile::ProfileCommand;
// pub use tools::ToolsCommand;
