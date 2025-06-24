use std::sync::Arc;

use fig_os_shim::Context;
use fig_settings::{
    Settings,
    State,
};

#[derive(Debug, Clone)]
pub struct CliContext {
    settings: Settings,
    state: State,
    context: Arc<Context>,
}

impl Default for CliContext {
    fn default() -> Self {
        Self::new()
    }
}

impl CliContext {
    pub fn new() -> Self {
        let settings = Settings::new();
        let state = State::new();
        let context = Context::new();

        Self {
            settings,
            state,
            context,
        }
    }

    pub fn new_fake() -> Self {
        let settings = Settings::new_fake();
        let state = State::new_fake();
        let context = Context::new_fake();

        Self {
            settings,
            state,
            context,
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn context(&self) -> &Context {
        &self.context
    }
}
