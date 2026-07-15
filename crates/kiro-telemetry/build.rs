fn main() {
    println!("cargo:rerun-if-env-changed=KIRO_VERSION");
    if let Ok(version) = std::env::var("KIRO_VERSION")
        && !version.is_empty()
    {
        println!("cargo:rustc-env=CARGO_PKG_VERSION={version}");
    } else if matches!(std::env::var("CARGO_PKG_VERSION").as_deref(), Ok("0.0.0-dev")) {
        println!("cargo:rustc-env=CARGO_PKG_VERSION=99.99.99-dev");
    }
}
