mod env;
mod fs;
mod platform;
pub mod process_info;
mod providers;
mod sysinfo;

use std::sync::Arc;

pub use env::Env;
pub use fs::Fs;
pub use platform::{
    Os,
    Platform,
};
use process_info::FakePid;
pub use process_info::ProcessInfo;
pub use providers::{
    ContextArcProvider,
    ContextProvider,
    EnvProvider,
    FsProvider,
    PlatformProvider,
    SysInfoProvider,
};
pub use sysinfo::SysInfo;

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
    sysinfo: SysInfo,
}

impl Context {
    /// Returns a new [Context] with real implementations of each OS shim.
    pub fn new() -> Arc<Self> {
        Arc::new_cyclic(|ctx| Self {
            fs: Default::default(),
            env: Default::default(),
            platform: Default::default(),
            process_info: ProcessInfo::new(ctx.clone()),
            sysinfo: SysInfo::default(),
        })
    }

    pub fn new_fake() -> Arc<Self> {
        Arc::new(Self {
            fs: Fs::new_fake(),
            env: Env::new_fake(),
            platform: Platform::new_fake(Os::current()),
            process_info: ProcessInfo::new_fake(FakePid::default()),
            sysinfo: SysInfo::new_fake(),
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

    pub fn sysinfo(&self) -> &SysInfo {
        &self.sysinfo
    }
}

#[derive(Default, Debug)]
pub struct ContextBuilder {
    fs: Option<Fs>,
    env: Option<Env>,
    platform: Option<Platform>,
    process_info: Option<ProcessInfo>,
    sysinfo: Option<SysInfo>,
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
        let sysinfo = self.sysinfo.unwrap_or_default();
        Arc::new_cyclic(|ctx| Context {
            fs,
            env,
            platform,
            process_info: if let Some(process_info) = self.process_info {
                process_info
            } else {
                ProcessInfo::new(ctx.clone())
            },
            sysinfo,
        })
    }

    /// Builds an immutable [Context] using fake implementations for each field by default.
    pub fn build_fake(self) -> Arc<Context> {
        let fs = self.fs.unwrap_or(Fs::new_fake());
        let env = self.env.unwrap_or(Env::new_fake());
        let platform = self.platform.unwrap_or(Platform::new_fake(Os::Mac));
        let sysinfo = self.sysinfo.unwrap_or(SysInfo::new_fake());
        Arc::new_cyclic(|ctx| Context {
            fs,
            env,
            platform,
            process_info: if let Some(process_info) = self.process_info {
                process_info
            } else {
                ProcessInfo::new(ctx.clone())
            },
            sysinfo,
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

    /// Creates a chroot filesystem and fake environment so that `$HOME`
    /// points to `<tempdir>/home/testuser`. Note that this replaces the
    /// [Fs] and [Env] currently set with the builder.
    pub async fn with_test_home(mut self) -> Result<Self, std::io::Error> {
        let home = "/home/testuser";
        let fs = Fs::new_chroot();
        fs.create_dir_all(home).await?;
        self.fs = Some(fs);
        self.env = Some(Env::from_slice(&[("HOME", "/home/testuser"), ("USER", "testuser")]));
        Ok(self)
    }

    pub fn with_env_var(mut self, key: &str, value: &str) -> Self {
        self.env = match self.env {
            Some(env) if !env.is_real() => {
                unsafe { env.set_var(key, value) };
                Some(env)
            },
            _ => Some(Env::from_slice(&[(key, value)])),
        };
        self
    }

    pub fn with_os(mut self, os: Os) -> Self {
        self.platform = Some(Platform::new_fake(os));
        self
    }

    pub fn with_running_processes(mut self, process_names: &[&str]) -> Self {
        let sysinfo = match self.sysinfo {
            Some(sysinfo) if !sysinfo.is_real() => sysinfo,
            _ => SysInfo::new_fake(),
        };
        sysinfo.add_running_processes(process_names);
        self.sysinfo = Some(sysinfo);
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
        assert!(ctx.sysinfo().is_real());
    }

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
