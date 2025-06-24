#![allow(dead_code)]
#![allow(unused_variables)]

pub mod diagnostics;
mod env;
mod fs;
mod os;
mod providers;
mod sysinfo;

use std::sync::Arc;

pub use env::Env;
pub use fs::Fs;
pub use os::{
    Os,
    Platform,
};
pub use providers::{
    EnvProvider,
    FsProvider,
    SysInfoProvider,
};
pub use sysinfo::SysInfo;

/// Struct that contains the interface to every system related IO operation.
///
/// Every operation that accesses the file system, environment, or other related platform
/// primitives should be done through a [Context] as this enables testing otherwise untestable
/// code paths in unit tests.
#[derive(Debug, Clone)]
pub struct Context {
    fs: Fs,
    env: Env,
    sysinfo: SysInfo,
    platform: Platform,
}

impl Context {
    /// Returns a new [Context] with real implementations of each OS shim.
    pub fn new() -> Arc<Self> {
        match cfg!(test) {
            true => Arc::new(Self {
                fs: Fs::new(),
                env: Env::new(),
                sysinfo: SysInfo::new(),
                platform: Platform::new(),
            }),
            false => Arc::new_cyclic(|_| Self {
                fs: Default::default(),
                env: Default::default(),
                sysinfo: SysInfo::default(),
                platform: Platform::new(),
            }),
        }
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

    pub fn sysinfo(&self) -> &SysInfo {
        &self.sysinfo
    }

    pub fn platform(&self) -> &Platform {
        &self.platform
    }
}

#[derive(Default, Debug)]
pub struct ContextBuilder {
    fs: Option<Fs>,
    env: Option<Env>,
    sysinfo: Option<SysInfo>,
    platform: Option<Platform>,
}

impl ContextBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds an immutable [Context] using real implementations for each field by default.
    pub fn build(self) -> Arc<Context> {
        let fs = self.fs.unwrap_or_default();
        let env = self.env.unwrap_or_default();
        let sysinfo = self.sysinfo.unwrap_or_default();
        let platform = self.platform.unwrap_or_default();
        Arc::new_cyclic(|_| Context {
            fs,
            env,
            sysinfo,
            platform,
        })
    }

    /// Builds an immutable [Context] using fake implementations for each field by default.
    pub fn build_fake(self) -> Arc<Context> {
        let fs = self.fs.unwrap_or_default();
        let env = self.env.unwrap_or_default();
        let sysinfo = self.sysinfo.unwrap_or_default();
        let platform = self.platform.unwrap_or_default();
        Arc::new_cyclic(|_| Context {
            fs,
            env,
            sysinfo,
            platform,
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

    /// Creates a chroot filesystem and fake environment so that `$HOME`
    /// points to `<tempdir>/home/testuser`. Note that this replaces the
    /// [Fs] and [Env] currently set with the builder.
    #[cfg(test)]
    pub async fn with_test_home(mut self) -> Result<Self, std::io::Error> {
        let home = "/home/testuser";
        let fs = Fs::new_chroot();
        fs.create_dir_all(home).await?;
        self.fs = Some(fs);
        self.env = Some(Env::from_slice(&[("HOME", "/home/testuser"), ("USER", "testuser")]));
        Ok(self)
    }

    #[cfg(test)]
    pub fn with_env_var(mut self, key: &str, value: &str) -> Self {
        self.env = match self.env {
            Some(env) if cfg!(test) => {
                unsafe { env.set_var(key, value) };
                Some(env)
            },
            _ => Some(Env::from_slice(&[(key, value)])),
        };
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_builder_with_test_home() {
        let ctx = ContextBuilder::new()
            .with_test_home()
            .await
            .unwrap()
            .with_env_var("hello", "world")
            .build();
        assert!(ctx.fs().try_exists("/home/testuser").await.unwrap());
        assert_eq!(ctx.env().get("HOME").unwrap(), "/home/testuser");
        assert_eq!(ctx.env().get("hello").unwrap(), "world");
    }
}
