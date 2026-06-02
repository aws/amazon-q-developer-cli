fn main() {
    inject_kiro_version();
}

/// Resolve the CLI version reported by `env!("CARGO_PKG_VERSION")` callsites,
/// overriding the `0.0.0-dev` placeholder shipped in the workspace `Cargo.toml`.
///
/// The placeholder exists for the tag-driven release model (PR #2570): CI
/// injects the real version via `KIRO_VERSION` (or by sed-ing Cargo.toml).
/// Local builds set neither — and KRS gates "thinking" on appVersion >= 2.4.0,
/// so a bare `cargo build` would silently disable thinking on every dev session.
///
/// Resolution order:
///   1. `KIRO_VERSION` env var (CI / explicit override)
///   2. Latest reachable git tag, e.g. `v2.6.0-rc.1` -> `2.6.0-rc.1+dev`
///   3. Hardcoded `99.99.99-dev` (sparse checkout, tarball, no git)
fn inject_kiro_version() {
    println!("cargo:rerun-if-env-changed=KIRO_VERSION");
    println!("cargo:rerun-if-changed=../../.git/HEAD");

    let resolved = std::env::var("KIRO_VERSION")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(git_describe_fallback)
        .unwrap_or_else(|| "99.99.99-dev".to_string());

    println!("cargo:rustc-env=CARGO_PKG_VERSION={resolved}");
}

fn git_describe_fallback() -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let tag = String::from_utf8(out.stdout)
        .ok()?
        .trim()
        .trim_start_matches('v')
        .to_string();
    if tag.is_empty() { None } else { Some(format!("{tag}+dev")) }
}
