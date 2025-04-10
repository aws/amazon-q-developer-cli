use objc2_foundation::{
    NSOperatingSystemVersion,
    NSProcessInfo,
};

#[derive(Clone, Copy, Debug)]
pub struct OperatingSystemVersion(NSOperatingSystemVersion);

impl OperatingSystemVersion {
    pub fn get() -> Self {
        Self(NSProcessInfo::processInfo().operatingSystemVersion())
    }

    pub fn major(&self) -> isize {
        self.0.majorVersion
    }

    pub fn minor(&self) -> isize {
        self.0.minorVersion
    }

    pub fn patch(&self) -> isize {
        self.0.patchVersion
    }
}

impl std::fmt::Display for OperatingSystemVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major(), self.minor(), self.patch())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_operating_system_version() {
        let v = OperatingSystemVersion::get();
        assert!(v.major() >= 10);

        let formatted = format!("{v}");
        println!("version: {formatted}");
        assert!(formatted.contains('.'));
    }
}
