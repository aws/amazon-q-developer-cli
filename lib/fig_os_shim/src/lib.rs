mod env;
mod fs;
mod platform;
pub mod process_info;
mod providers;

use std::sync::Arc;

pub use env::Env;
pub use fs::Fs;
pub use platform::{
    Os,
    Platform,
};
pub use process_info::ProcessInfo;
pub use providers::{
    EnvProvider,
    FsProvider,
};

pub trait Shim {
    /// Returns whether or not the shim is a real implementation.
    fn is_real(&self) -> bool;
}

/// Struct that contains the interface to every system related IO operation.
///
/// Every operation that accesses the file system, environment, or other related platform
/// primitives should be done through a [Context] as this enables testing otherwise untestable
/// code paths in unit tests.
#[derive(Debug, Clone)]
pub struct Context {
    #[allow(dead_code)]
    fs: Fs,
    env: Env,
    platform: Platform,
    process_info: ProcessInfo,
}

impl Context {
    /// Returns a new [Context] with real implementations of each OS shim.
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|ctx| Self {
            fs: Default::default(),
            env: Default::default(),
            platform: Default::default(),
            process_info: ProcessInfo::new(ctx.clone()),
        })
    }

    pub fn builder() -> ContextBuilder {
        ContextBuilder::new()
    }

    pub fn fs(&self) -> &Fs {
        &self.fs
    }

    pub fn env(&self) -> &Env {
        &self.env
    }

    pub fn platform(&self) -> &Platform {
        &self.platform
    }

    pub fn process_info(&self) -> &ProcessInfo {
        &self.process_info
    }
}

#[derive(Default, Debug)]
pub struct ContextBuilder {
    fs: Option<Fs>,
    env: Option<Env>,
    platform: Option<Platform>,
    process_info: Option<ProcessInfo>,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds an immutable [Context] using real implementations for each field by default.
    pub fn build(self) -> Arc<Context> {
        let fs = self.fs.unwrap_or_default();
        let env = self.env.unwrap_or_default();
        let platform = self.platform.unwrap_or_default();
        Arc::new_cyclic(|ctx| Context {
            fs,
            env,
            platform,
            process_info: if let Some(process_info) = self.process_info {
                process_info
            } else {
                ProcessInfo::new(ctx.clone())
            },
        })
    }

    pub fn with_env(mut self, env: Env) -> Self {
        self.env = Some(env);
        self
    }

    pub fn with_fs(mut self, fs: Fs) -> Self {
        self.fs = Some(fs);
        self
    }

    pub fn with_platform(mut self, platform: Platform) -> Self {
        self.platform = Some(platform);
        self
    }

    pub fn with_process_info(mut self, process_info: ProcessInfo) -> Self {
        self.process_info = Some(process_info);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_builder_returns_real_impls_by_default() {
        let ctx = ContextBuilder::new().build();
        assert!(ctx.fs().is_real());
        assert!(ctx.env().is_real());
        assert!(ctx.process_info().is_real());
        assert!(ctx.platform().is_real());
    }
}