use std::sync::Arc;

use crate::platform::Context;

#[derive(Debug, Clone)]
pub struct CliContext {
    context: Arc<Context>,
}

impl Default for CliContext {
    fn default() -> Self {
        Self::new()
    }
}

impl CliContext {
    pub fn new() -> Self {
        Self {
            context: Context::new(),
        }
    }

    pub fn context(&self) -> &Context {
        &self.context
    }
}
