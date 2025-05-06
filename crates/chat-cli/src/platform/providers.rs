use std::sync::Arc;

use crate::platform::{
    Context,
    Env,
    Fs,
    SysInfo,
};

pub trait ContextProvider {
    fn context(&self) -> &Context;
}

pub trait ContextArcProvider {
    fn context_arc(&self) -> Arc<Context>;
}

impl ContextArcProvider for Arc<Context> {
    fn context_arc(&self) -> Arc<Context> {
        Arc::clone(self)
    }
}

macro_rules! impl_context_provider {
    ($a:ty) => {
        impl ContextProvider for $a {
            fn context(&self) -> &Context {
                self
            }
        }
    };
}

impl_context_provider!(Arc<Context>);
impl_context_provider!(&Arc<Context>);
impl_context_provider!(Context);
impl_context_provider!(&Context);

pub trait EnvProvider {
    fn env(&self) -> &Env;
}

impl EnvProvider for Env {
    fn env(&self) -> &Env {
        self
    }
}

impl<T> EnvProvider for T
where
    T: ContextProvider,
{
    fn env(&self) -> &Env {
        self.context().env()
    }
}

pub trait FsProvider {
    fn fs(&self) -> &Fs;
}

impl FsProvider for Fs {
    fn fs(&self) -> &Fs {
        self
    }
}

impl<T> FsProvider for T
where
    T: ContextProvider,
{
    fn fs(&self) -> &Fs {
        self.context().fs()
    }
}
pub trait SysInfoProvider {
    fn sysinfo(&self) -> &SysInfo;
}

impl SysInfoProvider for SysInfo {
    fn sysinfo(&self) -> &SysInfo {
        self
    }
}

impl<T> SysInfoProvider for T
where
    T: ContextProvider,
{
    fn sysinfo(&self) -> &SysInfo {
        self.context().sysinfo()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_env_provider() {
        let env = Env::default();
        let env_provider = &env as &dyn EnvProvider;
        env_provider.env();
    }

    #[test]
    fn test_fs_provider() {
        let fs = Fs::default();
        let fs_provider = &fs as &dyn FsProvider;
        fs_provider.fs();
    }
}
