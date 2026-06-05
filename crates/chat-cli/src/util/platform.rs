/// Returns the system glibc version as (major, minor), or None if not detectable.
#[cfg(target_os = "linux")]
pub fn glibc_version() -> Option<(u32, u32)> {
    let output = std::process::Command::new("ldd").arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let first_line = text.lines().next().unwrap_or("");
    for token in first_line.rsplit(' ') {
        if let Some((major, minor)) = token.split_once('.') {
            if let (Ok(maj), Ok(min)) = (major.parse::<u32>(), minor.parse::<u32>()) {
                return Some((maj, min));
            }
        }
    }
    None
}

/// Minimum glibc version required by the embedded Node binary (Node 22).
#[cfg(target_os = "linux")]
pub const MIN_GLIBC_FOR_KAS: (u32, u32) = (2, 28);

/// Returns true if the current system can run KAS (has glibc >= 2.28).
#[cfg(target_os = "linux")]
pub fn can_run_kas() -> bool {
    match glibc_version() {
        Some((major, minor)) => (major, minor) >= MIN_GLIBC_FOR_KAS,
        None => true,
    }
}

#[cfg(not(target_os = "linux"))]
pub fn can_run_kas() -> bool {
    true
}
