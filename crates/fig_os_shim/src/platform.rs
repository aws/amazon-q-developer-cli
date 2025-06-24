use std::fmt;

use serde::Serialize;

use crate::Shim;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub enum Os {
    Mac,
    Linux,
    Windows,
}

impl Os {
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            return Self::Mac;
        }

        #[cfg(target_os = "linux")]
        {
            return Self::Linux;
        }

        #[cfg(target_os = "windows")]
        {
            return Self::Windows;
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            compile_error!("unsupported platform");
        }

        // This line should never be reached due to the compile_error above,
        // but it's needed to satisfy the compiler
        #[allow(unreachable_code)]
        {
            panic!("unsupported platform");
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Mac, Self::Linux, Self::Windows]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Mac => "macos",
            Self::Linux => "linux",
            Self::Windows => "windows",
        }
    }
}

impl fmt::Display for Os {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Default, Debug, Clone)]
pub struct Platform(inner::Inner);

mod inner {
    use super::*;

    #[derive(Default, Debug, Clone)]
    pub(super) enum Inner {
        #[default]
        Real,
        Fake(Os),
    }
}

impl Platform {
    /// Returns a real implementation of [Platform].
    pub fn new() -> Self {
        Self(inner::Inner::Real)
    }

    /// Returns a new fake [Platform].
    pub fn new_fake(os: Os) -> Self {
        Self(inner::Inner::Fake(os))
    }

    /// Returns the current [Os].
    pub fn os(&self) -> Os {
        use inner::Inner;
        match &self.0 {
            Inner::Real => Os::current(),
            Inner::Fake(os) => *os,
        }
    }
}

impl Shim for Platform {
    fn is_real(&self) -> bool {
        matches!(self.0, inner::Inner::Real)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform() {
        let platform = Platform::default();
        assert!(platform.is_real());

        for os in Os::all() {
            let platform = Platform::new_fake(*os);
            assert!(!platform.is_real());
            assert_eq!(&platform.os(), os);

            let _ = os.as_str();
            println!("{os:?} {os}");
        }
    }
}
