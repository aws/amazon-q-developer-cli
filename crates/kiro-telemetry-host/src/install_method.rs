use std::path::{
    Path,
    PathBuf,
};
use std::process::Command;
use std::sync::{
    Mutex,
    MutexGuard,
    PoisonError,
};
use std::time::Instant;

use serde::{
    Deserialize,
    Serialize,
};
use tracing::debug;

/// Cask name (its `Caskroom` metadata directory) and CLI binary name. Both are
/// `kiro-cli`; this crate is kiro-cli specific.
const CLI_BINARY_NAME: &str = "kiro-cli";

/// Homebrew prefixes for the default install locations (Apple Silicon, Intel
/// macOS, Linux/WSL). A custom `HOMEBREW_PREFIX`/`HOMEBREW_CASKROOM` is consulted
/// at runtime in addition to these.
const HOMEBREW_PREFIXES: &[&str] = &["/opt/homebrew", "/usr/local", "/home/linuxbrew/.linuxbrew"];

/// Process-wide cache of the resolved install method.
static INSTALL_METHOD: Mutex<Cache> = Mutex::new(Cache::new());

/// Cached install method plus whether it was resolved by the accurate path.
#[derive(Debug, Default, Clone)]
struct Cache {
    method: Option<InstallMethod>,
    is_accurate: bool,
}

impl Cache {
    const fn new() -> Self {
        Self {
            method: None,
            is_accurate: false,
        }
    }
}

/// The method used to install the CLI
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallMethod {
    Brew,
    Toolbox(String),
    Unknown,
}

impl std::fmt::Display for InstallMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallMethod::Brew => f.write_str("brew"),
            InstallMethod::Toolbox(v) if v.is_empty() => f.write_str("toolbox"),
            InstallMethod::Toolbox(v) => write!(f, "toolbox ({v})"),
            InstallMethod::Unknown => f.write_str("unknown"),
        }
    }
}

/// Inputs to [`detect_install_method`]. Production callers use the defaults;
/// tests inject the rest to exercise detection without touching real
/// subprocesses, the filesystem, or the environment.
#[derive(Debug, Default)]
struct DetectArgs {
    /// Use the authoritative `brew list --cask` / `toolbox list` subprocesses
    /// rather than the subprocess-free heuristics. Set by the resolver fns.
    use_slow_accurate_method: bool,
    /// Root searched for the cask's `Caskroom` metadata on the fast path. When
    /// `None`, the default Homebrew prefixes and `HOMEBREW_*` env vars are used.
    base: Option<PathBuf>,
    /// Injected `toolbox list --installed` output; when `None`, the subprocess runs.
    toolbox_list_output: Option<String>,
    /// Injected `brew list --cask` output; when `None`, the subprocess runs.
    brew_list_output: Option<String>,
    /// Injected executable path; when `None`, the real current exe is resolved.
    current_exe_path: Option<PathBuf>,
}

/// Return the install method using only subprocess-free checks. Prefers the
/// accurate result once [`get_accurate_install_method`] has resolved it,
/// otherwise computes (and caches) the coarse value from the executable path and
/// a `Caskroom` stat. Safe on the startup critical path.
pub fn get_install_method() -> InstallMethod {
    get_install_method_impl(&INSTALL_METHOD, DetectArgs::default())
}

/// Resolve the install method authoritatively, using the `brew list --cask` /
/// `toolbox list` subprocesses (~25s on some hosts for toolbox), and cache it.
/// For latency-tolerant callers (e.g. `diagnostics` and the long-lived session
/// pre-warm); hot paths must use [`get_install_method`].
pub fn get_accurate_install_method() -> InstallMethod {
    get_accurate_install_method_impl(&INSTALL_METHOD, DetectArgs::default())
}

/// Fast resolver: returns any cached value (a previously resolved accurate value
/// is preferred), otherwise detects coarsely and memoizes it as non-accurate.
///
/// The lock is never held across detection, so this never blocks for long even
/// if the accurate resolver is mid-probe on another thread.
fn get_install_method_impl(cache: &Mutex<Cache>, mut args: DetectArgs) -> InstallMethod {
    args.use_slow_accurate_method = false;

    if let Some(method) = lock(cache).method.clone() {
        return method;
    }

    let start = Instant::now();
    let method = detect_install_method(args);
    debug!(install_method = %method, elapsed_ms = start.elapsed().as_millis(), "detected install method (fast)");

    // Don't clobber an accurate value a concurrent caller may have stored.
    lock(cache).method.get_or_insert(method).clone()
}

/// Accurate resolver: returns the cached value only if it was resolved
/// accurately, otherwise runs the authoritative probe and caches it.
///
/// The lock is released before the (potentially slow) probe and re-acquired to
/// store, so the fast resolver never blocks on it.
fn get_accurate_install_method_impl(cache: &Mutex<Cache>, mut args: DetectArgs) -> InstallMethod {
    args.use_slow_accurate_method = true;

    {
        let cached = lock(cache);
        if cached.is_accurate
            && let Some(method) = cached.method.clone()
        {
            return method;
        }
    }

    let start = Instant::now();
    let method = detect_install_method(args);
    debug!(install_method = %method, elapsed_ms = start.elapsed().as_millis(), "detected install method (accurate)");

    let mut cached = lock(cache);
    cached.method = Some(method.clone());
    cached.is_accurate = true;
    method
}

fn lock(cache: &Mutex<Cache>) -> MutexGuard<'_, Cache> {
    cache.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Detect the install method from the given [`DetectArgs`].
///
/// Toolbox is detected from the executable path (a `.toolbox` component); its
/// version is only resolved on the accurate path. Brew is detected from the cask,
/// not the binary path: the cask installs `Kiro CLI.app` into the applications
/// dir and symlinks the binary inside the bundle, so the path carries no Homebrew
/// marker (and a brew install is identical on disk to a direct download). On the
/// fast path brew is a `Caskroom` directory stat; on the accurate path it is
/// `brew list --cask`. Toolbox wins when both are present.
fn detect_install_method(args: DetectArgs) -> InstallMethod {
    let exe = args.current_exe_path.unwrap_or_else(|| {
        std::env::current_exe()
            .map(|exe| std::fs::canonicalize(&exe).unwrap_or(exe))
            .unwrap_or_default()
    });

    // First, detect toolbox.
    if exe.components().any(|c| c.as_os_str() == ".toolbox") {
        let version = if args.use_slow_accurate_method {
            let toolbox_list_output = args.toolbox_list_output.or_else(|| {
                Command::new("toolbox")
                    .args(["list", "--installed"])
                    .output()
                    .ok()
                    .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            });
            parse_toolbox_version(toolbox_list_output).unwrap_or_default()
        } else {
            // No version without the (slow) accurate probe.
            String::new()
        };
        return InstallMethod::Toolbox(version);
    }

    // Then, detect brew.
    let brew_installed = if args.use_slow_accurate_method {
        let brew_list_output = args.brew_list_output.or_else(|| {
            Command::new("brew")
                .args(["list", "--cask"])
                .output()
                .ok()
                .map(|output| {
                    if output.status.success() {
                        String::from_utf8_lossy(&output.stdout).into_owned()
                    } else {
                        Default::default()
                    }
                })
        });
        is_brew_cask_listed(brew_list_output)
    } else {
        is_brew_caskroom_present(args.base.as_deref())
    };
    if brew_installed {
        return InstallMethod::Brew;
    }

    // Other install methods unknown. Currently we can't distinguish public S3
    // downloads from e.g. local development builds.
    InstallMethod::Unknown
}

/// Resolve the Toolbox version from `toolbox list --installed` output. The
/// `kiro-cli` row looks like `kiro-cli   2.8.1-beta(beta)   2026-06-18 00:42`.
fn parse_toolbox_version(output: Option<String>) -> Option<String> {
    output?
        .lines()
        .find(|line| line.starts_with(CLI_BINARY_NAME))
        .and_then(|line| line.split_whitespace().nth(1))
        .map(str::to_string)
}

/// Whether the cask appears in `brew list --cask` output (one cask token per line).
fn is_brew_cask_listed(output: Option<String>) -> bool {
    let Some(output) = output else { return false };
    output.lines().any(|line| line.trim() == CLI_BINARY_NAME)
}

/// Whether the cask's `Caskroom/<cask>` metadata directory exists (a subprocess-
/// free stat, for the fast path). `base` overrides the search root; when `None`,
/// the default Homebrew prefixes and `HOMEBREW_*` env vars are searched.
fn is_brew_caskroom_present(base: Option<&Path>) -> bool {
    let caskrooms = match base {
        Some(base) => vec![base.join("Caskroom")],
        None => default_caskrooms(),
    };
    caskrooms.iter().any(|caskroom| caskroom.join(CLI_BINARY_NAME).is_dir())
}

fn default_caskrooms() -> Vec<PathBuf> {
    let mut caskrooms: Vec<PathBuf> = HOMEBREW_PREFIXES
        .iter()
        .map(|prefix| PathBuf::from(prefix).join("Caskroom"))
        .collect();
    if let Ok(prefix) = std::env::var("HOMEBREW_PREFIX")
        && !prefix.is_empty()
    {
        caskrooms.push(PathBuf::from(prefix).join("Caskroom"));
    }
    if let Ok(caskroom) = std::env::var("HOMEBREW_CASKROOM")
        && !caskroom.is_empty()
    {
        caskrooms.push(PathBuf::from(caskroom));
    }
    caskrooms
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trimmed-down sample of real `toolbox list --installed` output.
    const TEST_TOOLBOX_OUTPUT_WITH_BETA_INSTALL: &str = "\
Tool           Current Version    Released (UTC)   Forced OS
----           ---------------    --------------   ---------
ada            1.0.202218.0       2026-05-27 15:19
kiro-cli       2.8.1-beta(beta)   2026-06-18 00:42
toolbox        1.1.5817.0         2026-06-02 19:30
";

    /// Same, but without a `kiro-cli` row.
    const TEST_TOOLBOX_OUTPUT_WITHOUT_INSTALL: &str = "\
Tool           Current Version    Released (UTC)   Forced OS
----           ---------------    --------------   ---------
ada            1.0.202218.0       2026-05-27 15:19
";

    /// Sample `brew list --cask` output: one cask token per line.
    const TEST_BREW_CASK_OUTPUT_WITH_INSTALL: &str = "\
ghostty
google-chrome
kiro-cli
visual-studio-code
";

    /// Same, but without the `kiro-cli` cask.
    const TEST_BREW_CASK_OUTPUT_WITHOUT_INSTALL: &str = "\
ghostty
google-chrome
visual-studio-code
";

    const TOOLBOX_EXE: &str = "/Users/someone/.toolbox/tools/kiro-cli/2.8.1/kiro-cli";
    const APP_EXE: &str = "/Applications/Kiro CLI.app/Contents/MacOS/kiro-cli-chat";

    fn beta_version() -> InstallMethod {
        InstallMethod::Toolbox("2.8.1-beta(beta)".to_string())
    }

    /// A temp dir, optionally seeded with `Caskroom/kiro-cli` as a brew host has.
    fn base_with_caskroom(present: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        if present {
            std::fs::create_dir_all(dir.path().join("Caskroom").join(CLI_BINARY_NAME)).unwrap();
        }
        dir
    }

    struct DetectCase {
        name: &'static str,
        accurate: bool,
        exe: &'static str,
        toolbox_output: Option<&'static str>,
        brew_output: Option<&'static str>,
        caskroom_present: bool,
        expected: InstallMethod,
    }

    #[test]
    fn detect_install_method_cases() {
        let cases = [
            DetectCase {
                name: "toolbox detected from path, version-less on the fast path",
                accurate: false,
                exe: TOOLBOX_EXE,
                toolbox_output: None,
                brew_output: None,
                caskroom_present: false,
                expected: InstallMethod::Toolbox(String::new()),
            },
            DetectCase {
                name: "toolbox version resolved on the accurate path",
                accurate: true,
                exe: TOOLBOX_EXE,
                toolbox_output: Some(TEST_TOOLBOX_OUTPUT_WITH_BETA_INSTALL),
                brew_output: None,
                caskroom_present: false,
                expected: beta_version(),
            },
            DetectCase {
                name: "toolbox version empty when not listed",
                accurate: true,
                exe: TOOLBOX_EXE,
                toolbox_output: Some(TEST_TOOLBOX_OUTPUT_WITHOUT_INSTALL),
                brew_output: None,
                caskroom_present: false,
                expected: InstallMethod::Toolbox(String::new()),
            },
            DetectCase {
                name: "toolbox takes precedence over brew",
                accurate: false,
                exe: TOOLBOX_EXE,
                toolbox_output: None,
                brew_output: None,
                caskroom_present: true,
                expected: InstallMethod::Toolbox(String::new()),
            },
            DetectCase {
                name: "brew detected via caskroom on the fast path",
                accurate: false,
                exe: APP_EXE,
                toolbox_output: None,
                brew_output: None,
                caskroom_present: true,
                expected: InstallMethod::Brew,
            },
            DetectCase {
                name: "unknown when caskroom absent on the fast path",
                accurate: false,
                exe: APP_EXE,
                toolbox_output: None,
                brew_output: None,
                caskroom_present: false,
                expected: InstallMethod::Unknown,
            },
            DetectCase {
                name: "brew detected via brew list on the accurate path",
                accurate: true,
                exe: APP_EXE,
                toolbox_output: None,
                brew_output: Some(TEST_BREW_CASK_OUTPUT_WITH_INSTALL),
                caskroom_present: false,
                expected: InstallMethod::Brew,
            },
            DetectCase {
                name: "unknown when cask absent from brew list",
                accurate: true,
                exe: APP_EXE,
                toolbox_output: None,
                brew_output: Some(TEST_BREW_CASK_OUTPUT_WITHOUT_INSTALL),
                caskroom_present: false,
                expected: InstallMethod::Unknown,
            },
        ];

        for case in cases {
            let base = base_with_caskroom(case.caskroom_present);
            let method = detect_install_method(DetectArgs {
                use_slow_accurate_method: case.accurate,
                base: Some(base.path().to_path_buf()),
                toolbox_list_output: case.toolbox_output.map(str::to_string),
                brew_list_output: case.brew_output.map(str::to_string),
                current_exe_path: Some(PathBuf::from(case.exe)),
            });
            assert_eq!(method, case.expected, "{}", case.name);
        }
    }

    #[derive(Clone, Copy)]
    enum Op {
        Fast,
        Accurate,
    }

    struct CacheCase {
        name: &'static str,
        ops: Vec<Op>,
        expected: Vec<InstallMethod>,
        final_is_accurate: bool,
    }

    #[test]
    fn cache_state_machine_cases() {
        let fast = InstallMethod::Toolbox(String::new());
        let accurate = beta_version();

        let cases = [
            CacheCase {
                name: "fast only caches a non-accurate value",
                ops: vec![Op::Fast],
                expected: vec![fast.clone()],
                final_is_accurate: false,
            },
            CacheCase {
                name: "accurate only caches an accurate value",
                ops: vec![Op::Accurate],
                expected: vec![accurate.clone()],
                final_is_accurate: true,
            },
            CacheCase {
                name: "fast then accurate still upgrades to accurate",
                ops: vec![Op::Fast, Op::Accurate],
                expected: vec![fast.clone(), accurate.clone()],
                final_is_accurate: true,
            },
            CacheCase {
                name: "accurate then fast returns the cached accurate value",
                ops: vec![Op::Accurate, Op::Fast],
                expected: vec![accurate.clone(), accurate.clone()],
                final_is_accurate: true,
            },
            CacheCase {
                name: "fast then fast stays non-accurate",
                ops: vec![Op::Fast, Op::Fast],
                expected: vec![fast.clone(), fast.clone()],
                final_is_accurate: false,
            },
            CacheCase {
                name: "accurate then accurate stays accurate",
                ops: vec![Op::Accurate, Op::Accurate],
                expected: vec![accurate.clone(), accurate.clone()],
                final_is_accurate: true,
            },
        ];

        for case in cases {
            let cache = Mutex::new(Cache::default());
            for (op, expected) in case.ops.iter().zip(&case.expected) {
                // A toolbox install: the fast path yields a version-less Toolbox,
                // the accurate path resolves the version from the injected output.
                let args = DetectArgs {
                    current_exe_path: Some(PathBuf::from(TOOLBOX_EXE)),
                    toolbox_list_output: Some(TEST_TOOLBOX_OUTPUT_WITH_BETA_INSTALL.to_string()),
                    ..Default::default()
                };
                let result = match op {
                    Op::Fast => get_install_method_impl(&cache, args),
                    Op::Accurate => get_accurate_install_method_impl(&cache, args),
                };
                assert_eq!(&result, expected, "{}", case.name);
            }
            assert_eq!(lock(&cache).is_accurate, case.final_is_accurate, "{}", case.name);
        }
    }

    #[test]
    fn display_omits_empty_toolbox_version() {
        assert_eq!(InstallMethod::Toolbox(String::new()).to_string(), "toolbox");
        assert_eq!(
            InstallMethod::Toolbox("2.7.1".to_string()).to_string(),
            "toolbox (2.7.1)"
        );
    }

    #[ignore = "hits the real system; run manually"]
    #[test]
    fn detect_install_method_e2e() {
        println!("fast:     {:?}", detect_install_method(DetectArgs::default()));
        println!(
            "accurate: {:?}",
            detect_install_method(DetectArgs {
                use_slow_accurate_method: true,
                ..Default::default()
            })
        );
    }
}
