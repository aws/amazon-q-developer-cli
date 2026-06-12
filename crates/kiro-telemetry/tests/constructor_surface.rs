use std::fs;
use std::path::{
    Path,
    PathBuf,
};

const PRODUCT_TELEMETRY_ROOTS: &[&str] = &["crates/chat-cli/src/telemetry", "crates/chat-cli-v2/src/telemetry"];

const FORBIDDEN_PRODUCT_PATTERNS: &[&str] = &[
    "MetricRecord::counter",
    "MetricRecord::counter_f64",
    "MetricRecord::histogram",
    "MetricRecord::gauge",
    "TelemetryLogRecord::new",
    ".with_attribute(",
    ".with_resource_attribute(",
    "metric::counter(",
    "metric::counter_f64(",
    "metric::histogram(",
    "metric::gauge(",
    "telemetry_log::event(",
];

#[test]
fn product_telemetry_uses_metric_and_log_constructors() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("kiro-telemetry crate should live under crates/");

    let mut violations = Vec::new();
    for root in PRODUCT_TELEMETRY_ROOTS {
        collect_violations(&repo_root.join(root), &mut violations);
    }

    assert!(
        violations.is_empty(),
        "product telemetry must use kiro_telemetry::metric and kiro_telemetry::log constructors:\n{}",
        violations.join("\n")
    );
}

fn collect_violations(path: &Path, violations: &mut Vec<String>) {
    if path.is_dir() {
        let mut entries = fs::read_dir(path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
            .map(|entry| entry.expect("failed to read directory entry").path())
            .collect::<Vec<_>>();
        entries.sort();
        for entry in entries {
            collect_violations(&entry, violations);
        }
        return;
    }

    if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
        return;
    }

    let contents = fs::read_to_string(path).unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
    for (line_index, line) in contents.lines().enumerate() {
        for pattern in FORBIDDEN_PRODUCT_PATTERNS {
            if line.contains(pattern) {
                violations.push(format!(
                    "{}:{} contains `{}`",
                    display_path(path),
                    line_index + 1,
                    pattern
                ));
            }
        }
    }
}

fn display_path(path: &Path) -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("kiro-telemetry crate should live under crates/");
    path.strip_prefix(repo_root).unwrap_or(path).display().to_string()
}
