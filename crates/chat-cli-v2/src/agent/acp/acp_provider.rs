use std::env::VarError;
use std::path::PathBuf;

use agent::util::providers::{
    CwdProvider,
    EnvProvider,
    HomeProvider,
    RealProvider,
    SystemProvider,
};

/// A [SystemProvider] for ACP sessions that uses a configured working directory
/// instead of the process's current directory.
#[derive(Debug, Clone)]
pub struct AcpProvider {
    cwd: PathBuf,
}

impl AcpProvider {
    pub fn new(cwd: PathBuf) -> Self {
        Self { cwd }
    }
}

impl EnvProvider for AcpProvider {
    fn var(&self, input: &str) -> Result<String, VarError> {
        RealProvider.var(input)
    }
}

impl HomeProvider for AcpProvider {
    fn home(&self) -> Option<PathBuf> {
        RealProvider.home()
    }
}

impl CwdProvider for AcpProvider {
    fn cwd(&self) -> Result<PathBuf, std::io::Error> {
        Ok(self.cwd.clone())
    }
}

impl SystemProvider for AcpProvider {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_returns_configured_cwd() {
        let provider = AcpProvider::new(PathBuf::from("/custom/path"));
        assert_eq!(provider.cwd().unwrap(), PathBuf::from("/custom/path"));
    }
}
